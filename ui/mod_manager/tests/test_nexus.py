from contextlib import contextmanager
import hashlib
import io
import json
from pathlib import Path
from types import SimpleNamespace
from urllib.error import HTTPError, URLError
from urllib.parse import urlsplit

import pytest

from creation_lib.mod_manager.nxm_instances import Inbox, forward_link
from creation_lib.mod_manager.browser_links import register_handler, protocol_command
from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.mo2 import inspect_instance
from creation_lib.mod_manager.nexus import NexusClient, NexusError, parse_nxm, validate_cdn_url, _Redirects
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from ui.toolkit.app import ToolkitApp
from .fixtures import make_mo2, make_package
from .test_controller import finish


LINK = "nxm://fallout4/mods/123/files/456?key=fixture-key&expires=4000000000&user_id=42"


class Response(io.BytesIO):
    def __init__(self, content, headers=None):
        super().__init__(content)
        self.headers = headers or {}


class Transport:
    def __init__(self, payload=b"fixture archive", *, premium=False):
        self.payload = payload
        self.premium = premium
        self.requests = []

    def __call__(self, request, timeout):
        self.requests.append(request)
        url = urlsplit(request.full_url)
        if url.hostname == "files.nexusmods.com":
            assert request.get_header("Apikey") is None
            return Response(self.payload, {"Content-Length": str(len(self.payload))})
        assert url.hostname == "api.nexusmods.com"
        assert request.get_header("Application-name") == "ModBox21"
        assert request.get_header("Application-version")
        if url.path.endswith("users/validate.json"):
            data = {"user_id": 42, "name": "Fixture account", "is_premium": self.premium}
        elif url.path.endswith("download_link.json"):
            data = [{"URI": "https://files.nexusmods.com/fixture.zip?ticket=fixture-ticket"}]
        elif "/files/" in url.path:
            data = {"file_name": "Fixture.zip", "version": "1.2", "md5": hashlib.md5(self.payload, usedforsecurity=False).hexdigest()}
        else:
            data = {"version": "1.3"}
        return Response(json.dumps(data).encode())


def connected(transport=None, premium=False):
    transport = transport or Transport(premium=premium)
    client = NexusClient("fixture-session-key", transport=transport)
    client.connect()
    return client


def test_nxm_validation_identity_and_redacted_representation():
    link = parse_nxm(LINK)
    assert link.identity == ("fallout4", 123, 456)
    assert link.key not in repr(link) and "user_id" not in repr(link)
    assert link.key not in link.label
    with pytest.raises(NexusError, match="expired"):
        parse_nxm("nxm://fallout4/mods/123/files/456?key=fixture&expires=1&user_id=42").check_expiry(now=100)


@pytest.mark.parametrize("url", [
    "https://fallout4/mods/1/files/2", "nxm://user@fallout4/mods/1/files/2", "nxm://fallout4:90/mods/1/files/2",
    "nxm://fallout4/mods/../files/2", "nxm://fallout4/collections/slug/revisions/1", "nxm://fallout4/mods/0/files/2",
    "nxm://fallout4/mods/1/files/2#fragment", "nxm://fallout4/mods/1/files/2?key=a&expires=2&key=b",
    "nxm://fallout4/mods/1/files/2?key=a", "nxm://fallout4/mods/1/files/2?key=%0A&expires=2",
    "nxm://fallout4/mods/1/files/2?unexpected=x", "nxm://fallout4/mods/1/files/2\n",
], ids=[f"invalid-{i}" for i in range(12)])
def test_invalid_links_fail_without_echoing_input(url):
    with pytest.raises(NexusError) as error:
        parse_nxm(url)
    assert url not in str(error.value)


def test_nexus_account_entitlement_and_game_mismatch():
    client = connected()
    with pytest.raises(NexusError, match="another Nexus account"):
        client.resolve(parse_nxm(LINK.replace("user_id=42", "user_id=43")))
    with pytest.raises(NexusError, match="Free accounts"):
        client.resolve(parse_nxm("nxm://fallout4/mods/123/files/456"))
    premium = connected(premium=True)
    metadata, _ = premium.resolve(parse_nxm("nxm://fallout4/mods/123/files/456"))
    assert metadata["file_name"] == "Fixture.zip"
    manager = ManagerController()
    try:
        manager.nexus_game = "skyrim"
        with pytest.raises(NexusError, match="mismatch"):
            manager.accept_nxm(LINK)
    finally:
        manager.cleanup()


@pytest.mark.parametrize("status", [401, 403, 404, 429, 500])
def test_api_failures_do_not_leak_keys_urls_or_response_body(status):
    def failed(request, timeout):
        raise HTTPError(request.full_url, status, "fixture secret body", {"Retry-After": "7200"}, io.BytesIO(b"private response"))
    client = NexusClient("fixture-session-key", transport=failed)
    with pytest.raises(NexusError) as error:
        client.connect()
    message = str(error.value)
    assert "fixture-session-key" not in message and "private response" not in message and "https://" not in message
    assert error.value.retry_after == 7200


def test_network_failure_and_untrusted_cdn_redirects_are_blocked():
    def failed(request, timeout):
        raise URLError("fixture private URL")
    with pytest.raises(NexusError, match="network"):
        NexusClient("fixture-session-key", transport=failed).connect()
    for url in ("http://files.nexusmods.com/a", "https://nexusmods.com.evil.example/a", "https://127.0.0.1/a", "https://user:secret@files.nexusmods.com/a"):
        with pytest.raises(NexusError):
            validate_cdn_url(url)
    from urllib.request import Request
    with pytest.raises(NexusError, match="credentials"):
        _Redirects(False).redirect_request(Request("https://api.nexusmods.com/v1/"), None, 302, "", {}, "https://files.nexusmods.com/a")
    with pytest.raises(NexusError):
        _Redirects(True).redirect_request(Request("https://files.nexusmods.com/a"), None, 302, "", {}, "https://evil.example/a")


def test_download_duplicate_cancel_failure_and_install_identity(tmp_path):
    payload = make_package(tmp_path).read_bytes()
    transport = Transport(payload)
    client = connected(transport)
    library = Library(tmp_path / "library", create=True)
    link = parse_nxm(LINK)
    path = Path(library.download_nexus(client, link))
    assert path.read_bytes() == payload
    before = len(transport.requests)
    assert library.download_nexus(client, link) == str(path)
    assert len(transport.requests) == before
    manager = ManagerController()
    manager.library = library
    manager.profile_id = library.create_profile("Main")
    manager.refresh()
    try:
        manager.prepare_install(path)
        finish(manager)
        manager.prepared.environment.files.update({"absent.esp": "Missing", "fixture.esp": "Missing"})
        manager.review_install()
        finish(manager)
        manager.enable_on_install = False
        manager.install("Nexus fixture")
        finish(manager)
        row = manager.rows[0]
        assert not row.enabled and row.nexus_mod_id == 123 and row.nexus_file_id == 456 and row.version == "1.2"
    finally:
        manager.cleanup()
    import threading
    cancel = threading.Event()
    next_link = parse_nxm(LINK.replace("/456?", "/457?"))
    with pytest.raises(Cancelled):
        library.download_nexus(client, next_link, cancel, lambda fraction, detail: cancel.set())
    assert len(library.downloads()) == 1 and not any((library.root / ".staging").iterdir())
    bad = connected(Transport(payload))
    bad.download = lambda *args, **kwargs: (_ for _ in ()).throw(NexusError("fixture download failure"))
    with pytest.raises(NexusError):
        library.download_nexus(bad, next_link)
    assert len(library.downloads()) == 1 and not any((library.root / ".staging").iterdir())
    assert b"fixture-session-key" not in library.database.read_bytes()


def test_download_length_checksum_and_filename_failure(tmp_path):
    client = connected(Transport())
    with pytest.raises(NexusError, match="checksum"):
        client.download("https://files.nexusmods.com/a", tmp_path / "bad.zip", md5="0" * 32)
    def transport(request, timeout):
        if urlsplit(request.full_url).hostname == "files.nexusmods.com":
            return Response(b"short", {"Content-Length": "100"})
        return Transport()(request, timeout)
    with pytest.raises(NexusError, match="complete"):
        connected(transport).download("https://files.nexusmods.com/a", tmp_path / "short.zip")


@pytest.mark.parametrize("download", [False, True])
def test_interrupted_response_errors_hide_transport_details(tmp_path, download):
    class Interrupted(Response):
        def read(self, size=-1):
            raise OSError("fixture-session-key private download URL")
    client = connected()
    client._transport = lambda request, timeout: Interrupted(b"")
    with pytest.raises(NexusError, match="interrupted") as error:
        if download:
            client.download("https://files.nexusmods.com/a", tmp_path / "interrupted.zip")
        else:
            client.mod_info("fallout4", 123)
    assert "fixture-session-key" not in str(error.value)


def test_real_local_inbox_forwards_to_running_instance_and_cleans_nonce(tmp_path):
    from creation_lib.mod_manager.nxm_instances import EMPTY
    context = {**EMPTY, "library_id": "fixture", "library_root": str(tmp_path), "profile_id": "fixture", "profile_name": "Fixture", "game": "fallout4", "ready": True}
    inbox = Inbox(tmp_path / "ipc", context=context)
    try:
        assert forward_link(tmp_path / "ipc", LINK)
        request = inbox.requests.get(timeout=3)
        assert request.identity == ("fallout4", 123, 456)
        assert request.key not in inbox.state.read_text()
        assert not forward_link(tmp_path / "absent", LINK)
    finally:
        inbox.close()
    assert not inbox.state.exists()
    assert not inbox.thread.is_alive()


def test_browser_cold_start_selects_standalone_manager_without_running_host(monkeypatch):
    import sys
    from ui.mod_manager import nxm_entry
    calls = []
    monkeypatch.setattr(sys, "argv", ["nxm_entry.py", LINK])
    monkeypatch.setattr(sys, "path", list(sys.path))
    monkeypatch.setattr("ui.toolkit.__main__.run_toolkit_variant", lambda variant, **kwargs: calls.append((variant, kwargs)))
    nxm_entry.main()
    assert calls == [("mod_manager", {"launch_path": LINK})]


def test_frozen_browser_command_selects_standalone_manager(monkeypatch):
    import sys
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    command = protocol_command()
    assert "--variant=mod_manager" in command and command.endswith(' "%1"')


def test_registry_handler_requires_opt_in_and_uses_safe_command():
    class Registry:
        HKEY_CURRENT_USER, HKEY_CLASSES_ROOT, REG_SZ = "user", "merged", "string"
        def __init__(self):
            self.writes = []
        @contextmanager
        def OpenKey(self, root, path):
            yield (root, path)
        def QueryValueEx(self, key, name):
            return ('"FixtureMO2.exe" "%1"', "string")
        @contextmanager
        def CreateKey(self, root, path):
            yield (root, path)
        def SetValueEx(self, key, name, reserved, type_, value):
            self.writes.append((key, name, value))
    registry = Registry()
    with pytest.raises(ManagerError, match="Confirm"):
        register_handler(registry=registry)
    assert not registry.writes
    previous = register_handler(confirmed=True, registry=registry)
    assert "FixtureMO2" in previous
    assert all(key[0] == "user" for key, _, _ in registry.writes)
    assert protocol_command().endswith(' "%1"')
    assert any("ModBox21.nxm" in key[1] for key, _, _ in registry.writes)


def test_instance_configuration_and_qt_executables_import_without_credentials(tmp_path):
    source = make_mo2(tmp_path)
    ini = source / "ModOrganizer.ini"
    with ini.open("a", encoding="utf-8") as stream:
        stream.write("enable_mods_after_install=false\nnexus_api_key=fixture-sensitive-key\n[customExecutables]\nsize=1\n1\\title=Fixture tool\n1\\binary=@ByteArray(C:\\\\Fixture\\\\Tool.exe)\n1\\arguments=--safe\n1\\workingDirectory=C:/Fixture\n1\\toolbar=true\n")
    meta = source / "mods" / "Base" / "meta.ini"
    with meta.open("a", encoding="utf-8") as stream:
        stream.write("modid=123\ngameName=Fallout 4\n")
    preview = inspect_instance(source)
    config = preview.configuration
    assert config["preferences"]["enable_mods_after_install"] == "false"
    assert config["executables"][0]["title"] == "Fixture tool"
    assert config["executables"][0]["arguments"] == "--safe"
    assert config["executable_launching"] is False
    assert "fixture-sensitive-key" not in json.dumps(config)
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(preview)
    assert library.configurations()[0]["configuration"] == config
    row = next(row for row in library.rows(library.profiles()[0].id) if row.name == "Base")
    assert (row.nexus_game, row.nexus_mod_id) == ("fallout4", 123)
    assert b"fixture-sensitive-key" not in library.database.read_bytes()


def test_visual_sort_nested_group_filter_does_not_change_priority(tmp_path):
    library = Library(tmp_path / "library", create=True)
    library.import_mo2(inspect_instance(make_mo2(tmp_path)))
    manager = ManagerController()
    manager.library = library
    try:
        manager.refresh()
        before = {row.id: row.priority for row in manager.rows}
        row = manager.rows[1]
        library.customize(manager.profile_id, row.id, group="Visual/Textures")
        manager.refresh()
        assert row.id in {item.id for item in manager.filtered(group="Visual")}
        names = [row.name for row in manager.filtered(sort_by="name")]
        assert names == sorted(names, key=str.casefold)
        assert {row.id: row.priority for row in library.rows(manager.profile_id)} == before
    finally:
        manager.cleanup()


def test_update_checks_can_be_disabled_are_bounded_and_back_off(tmp_path, monkeypatch):
    source = make_mo2(tmp_path)
    with (source / "mods" / "Base" / "meta.ini").open("a") as stream:
        stream.write("modid=123\ngameName=Fallout 4\n")
    manager = ManagerController()
    try:
        manager.open_library(tmp_path / "library", create=True)
        finish(manager)
        manager.inspect_mo2(source)
        finish(manager)
        manager.import_profiles({manager.import_preview.profiles[0].id})
        finish(manager)
        transport = Transport()
        manager.connect_nexus("fixture-session-key", factory=lambda key: NexusClient(key, transport=transport))
        finish(manager)
        assert manager.auto_updates
        manager.auto_updates = False
        manager.tick_updates()
        assert not manager.busy
        manager.auto_updates = True
        manager.tick_updates()
        finish(manager)
        assert manager.updates[("fallout4", 123)] == "1.3"
        assert len(transport.requests) == 2
        manager.tick_updates()
        assert not manager.busy
        manager._next_update = manager._rate_blocked_until = 0
        def limited(*args):
            raise NexusError("Nexus rate limit reached.", 7200)
        monkeypatch.setattr(manager.nexus_client, "mod_info", limited)
        manager.check_updates()
        with pytest.raises(NexusError):
            manager._future.result(timeout=5)
        manager.poll()
        assert manager._next_update > 0 and manager._rate_blocked_until == manager._next_update
        client = manager.nexus_client
        manager.disconnect_nexus()
        assert client._api_key == ""
    finally:
        manager.cleanup()


def test_startup_dispatch_and_logs_never_echo_download_tokens(caplog):
    host = ToolkitApp.__new__(ToolkitApp)
    host._launch_path, host._launch_open_done = LINK, False
    received = []
    workspace = SimpleNamespace(open_file=received.append, manager=SimpleNamespace(error=""))
    host._ws_map = {"mod_manager": workspace}
    host._switch_workspace = lambda *args, **kwargs: True
    host._handle_launch_open()
    assert len(received) == 1 and host._launch_path is None
    assert parse_nxm(LINK).key not in caplog.text
    assert ToolkitApp._resolve_launch_target(LINK)[0] == "mod_manager"


def test_duplicate_requests_refresh_tokens_and_queue_is_bounded():
    manager = ManagerController()
    try:
        manager.accept_nxm(LINK)
        manager.accept_nxm(LINK.replace("fixture-key", "fresh-fixture-key"))
        assert len(manager.pending_links) == 1
        assert next(iter(manager.pending_links.values())).key == "fresh-fixture-key"
        for file_id in range(500, 599):
            manager.accept_nxm(LINK.replace("/456?", f"/{file_id}?"))
        with pytest.raises(NexusError, match="queue is full"):
            manager.accept_nxm(LINK.replace("/456?", "/600?"))
    finally:
        manager.cleanup()


def test_update_batches_rotate_at_most_twenty_distinct_mods(tmp_path):
    manager = ManagerController()
    requests = []
    manager.rows = [SimpleNamespace(nexus_game="fallout4", nexus_mod_id=i) for i in range(1, 46)]
    manager.nexus_client = SimpleNamespace(mod_info=lambda game, mod: requests.append((game, mod)) or {"version": "2"}, forget=lambda: None)
    try:
        manager.check_updates()
        finish(manager)
        assert requests == [("fallout4", i) for i in range(1, 21)]
        manager._rate_blocked_until = 0
        manager.check_updates()
        finish(manager)
        assert requests[20:] == [("fallout4", i) for i in range(21, 41)]
        manager._rate_blocked_until = 0
        manager.check_updates()
        finish(manager)
        assert requests[40:45] == [("fallout4", i) for i in range(41, 46)]
        assert len(requests) == 60 and len(manager.updates) == 45
    finally:
        manager.cleanup()


@pytest.mark.parametrize("forwarded", [True, False])
def test_entrypoint_forwards_running_request_or_opens_full_toolkit(tmp_path, monkeypatch, forwarded):
    from ui.toolkit import __main__ as entry
    launched = []
    monkeypatch.setattr("creation_lib.mod_manager.nxm_instances.forward_link", lambda directory, raw: forwarded)
    monkeypatch.setattr("ui.toolkit.app_paths.get_ini_dir", lambda: tmp_path)
    monkeypatch.setattr(entry, "_set_taskbar_identity", lambda variant: None)
    monkeypatch.setattr(entry, "ToolkitSettings", lambda **kwargs: SimpleNamespace(setup_complete=True))
    monkeypatch.setattr(entry, "create_workspaces", lambda **kwargs: [])
    def host(workspaces, settings, **kwargs):
        launched.append(kwargs)
        return SimpleNamespace(run=lambda: None)
    monkeypatch.setattr(entry, "ToolkitApp", host)
    entry.run_toolkit_variant("nif", launch_path=LINK)
    assert len(launched) == (0 if forwarded else 1)
    if launched:
        assert launched[0]["app_variant"].id == "full" and launched[0]["launch_path"] == LINK
