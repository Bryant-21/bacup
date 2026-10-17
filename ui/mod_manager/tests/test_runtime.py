from dataclasses import replace
import json
import os
from pathlib import Path
import struct
import threading

import pytest

from creation_lib.mod_manager.launcher import Executable
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.runtime import prepare, profile_signature, start
from creation_lib.mod_manager.runtime_helper import run
from .fixtures import make_mo2
from creation_lib.mod_manager.mo2 import inspect_instance


def pe(path, machine=0x8664):
    content = bytearray(128)
    content[:2] = b"MZ"
    struct.pack_into("<I", content, 60, 64)
    content[64:68] = b"PE\0\0"
    struct.pack_into("<H", content, 68, machine)
    path.write_bytes(content)
    return path


@pytest.fixture
def fixture(tmp_path):
    library = Library(tmp_path / "library", create=True)
    source = make_mo2(tmp_path)
    library.import_mo2(inspect_instance(source))
    profile_id = library.profiles()[0].id
    game = tmp_path / "game"
    (game / "Data").mkdir(parents=True)
    binary = pe(game / "f4se_loader.exe")
    pe(game / "Fallout4.exe")
    bundle = tmp_path / "bundle"
    bundle.mkdir()
    for name, machine in (("usvfs_x64.dll", 0x8664), ("usvfs_x86.dll", 0x14c), ("usvfs_proxy_x64.exe", 0x8664), ("usvfs_proxy_x86.exe", 0x14c)):
        pe(bundle / name, machine)
    user = tmp_path / "game-user"
    (user / "plugins").mkdir(parents=True)
    (user / "ini").mkdir()
    executable = Executable("fixture-exe", "F4SE fixture", str(binary))
    return library, profile_id, executable, game, bundle, (user / "plugins", user / "ini")


def test_runtime_plan_winners_hidden_paths_snapshots_and_revalidation(fixture):
    library, profile_id, executable, game, bundle, targets = fixture
    rows = library.rows(profile_id)
    enabled = [row for row in rows if row.enabled]
    original = {path: path.read_bytes() for row in enabled for path in Path(row.source).rglob("*") if path.is_file()}
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    files = {name.casefold(): source for name, source, _ in plan.files}
    winner = next(row for row in rows if row.name == "Override")
    assert files["textures/shared.txt"] == str(Path(winner.source) / "textures/shared.txt")
    assert "hidden.mohidden" not in files and "meta.ini" not in files
    assert {kind for _, _, kind in plan.snapshots} == {"plugins", "ini"}
    assert not plan.isolate_documents
    assert all("SLocalSavePath = Saves\\" in text for name, text, kind in plan.snapshots if kind == "ini")
    assert all(path.read_bytes() == value for path, value in original.items())
    assert not (library.root / "runtime").exists()  # review makes no runtime directories
    with pytest.raises(ManagerError, match="acknowledge"):
        start(library, plan)
    library.customize(profile_id, winner.id, enabled=False)
    with pytest.raises(ManagerError, match="Profile changed"):
        start(library, plan, accept_source_writes=True, accept_minimal_ini=True)


@pytest.mark.parametrize("frozen", [False, True])
def test_default_managed_runtime_reaches_local_profile_plan_without_bundle_prompt(fixture, monkeypatch, frozen):
    import sys
    library, profile_id, executable, game, runtime_bundle, targets = fixture
    calls = []
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.ensure", lambda cancel, progress: calls.append("project") or runtime_bundle)
    if frozen:
        monkeypatch.setattr(sys, "frozen", True, raising=False)
        monkeypatch.setattr(sys, "_MEIPASS", str(game / "_internal"), raising=False)
    plan = prepare(library, profile_id, executable, str(game), "", user_targets=targets)
    assert plan.managed_runtime and plan.bundle_root == str(runtime_bundle)
    assert {str(game), str(targets[0])} <= set(plan.protected_roots)
    assert str(targets[1]) not in plan.protected_roots
    assert calls == ["project"] and plan.launch.profile_id == profile_id
    assert plan.runtime == plan.launch.runtime == "modbox-vfs-fo4-x64"
    assert not (library.root / "runtime").exists()


def test_frozen_onefile_refuses_before_provision_or_profile_io(fixture, monkeypatch):
    import sys
    library, profile_id, executable, game, _, targets = fixture
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "_MEIPASS", str(game / "_MEI_fixture"), raising=False)
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.ensure", lambda *args: pytest.fail("onefile build"))
    with pytest.raises(ManagerError, match="onedir"):
        prepare(library, profile_id, executable, str(game), "", user_targets=targets)


def test_runtime_omits_unavailable_plugins_but_rejects_tools_bundle_and_cancel(fixture):
    library, profile_id, executable, game, bundle, targets = fixture
    args = library, profile_id, executable, str(game), str(bundle)
    for changed in (replace(executable, binary=str(pe(game / "CreationKit.exe"))),
                    replace(executable, binary=str(pe(game / "Fallout4.exe", 0x14c)))):
        with pytest.raises(ManagerError, match="initial modded runtime"):
            prepare(library, profile_id, changed, str(game), str(bundle), user_targets=targets)
    pe(game / "Fallout4.exe")
    (bundle / "usvfs_x64.dll").unlink()
    with pytest.raises(ManagerError, match="component"):
        prepare(*args, user_targets=targets)
    pe(bundle / "usvfs_x64.dll")
    for row in library.rows(profile_id):
        if row.name == "Override" or row.name == "Base":
            library.customize(profile_id, row.id, enabled=False)
    plan = prepare(*args, user_targets=targets)
    plugins = next(text for name, text, _ in plan.snapshots if name.casefold() == "plugins.txt")
    assert not [line for line in plugins.splitlines() if line.startswith("*")]
    cancelled = threading.Event()
    cancelled.set()
    with pytest.raises(Cancelled):
        prepare(*args, cancelled, user_targets=targets)


def test_each_loader_directory_filters_only_its_launch_plugin_snapshots(fixture, tmp_path):
    library, profile_id, executable, game, bundle, targets = fixture
    other = tmp_path / "other installation"
    (other / "Data").mkdir(parents=True)
    pe(other / "f4se_loader.exe")
    pe(other / "Fallout4.exe")
    (game / "Data/OldDLC.esm").write_bytes(b"inert old DLC")
    (other / "Data/NewDLC.esm").write_bytes(b"inert new DLC")
    files = dict(library.profiles()[0].files)
    files["plugins.txt"] = "# generated fixture\r\n*OldDLC.esm\r\n*NewDLC.esm\r\n*Fixture.esp\r\n*Absent.esp\r\nDisabled.esp\r\n"
    files["loadorder.txt"] = "OldDLC.esm\nNewDLC.esm\nFixture.esp\nAbsent.esp\nDisabled.esp\n"
    files["lockedorder.txt"] = "OldDLC.esm|0\nNewDLC.esm|1\nFixture.esp|2\nAbsent.esp|3\n"
    with library._db() as db:
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile_id))
    for folder, present, absent in ((game, "OldDLC.esm", "NewDLC.esm"), (other, "NewDLC.esm", "OldDLC.esm"), (game, "OldDLC.esm", "NewDLC.esm")):
        chosen = replace(executable, binary=str(folder / "f4se_loader.exe"))
        plan = prepare(library, profile_id, chosen, str(game), str(bundle), user_targets=targets)
        assert plan.game_root == str(folder)
        for name, text, kind in plan.snapshots:
            if kind == "plugins":
                assert present in text and "Fixture.esp" in text
                assert absent not in text and "Absent.esp" not in text and "Disabled.esp" not in text
                if name == "plugins.txt":
                    assert text == f"# generated fixture\r\n*{present}\r\n*Fixture.esp\r\n"
    assert library.profiles()[0].files == files


class FakeVFS:
    def __init__(self, root, session):
        assert session.startswith("b21_") and session != "mod_organizer_instance"
        self.operations, self.closed, self.polls = [], False, 0

    def link_directory(self, source, target, *, create=False):
        self.operations.append(("directory", str(source), str(target), create))

    def link_file(self, source, target):
        self.operations.append(("file", str(source), str(target)))

    def start(self, argv, cwd, **kwargs):
        if kwargs.get("before_resume"):
            kwargs["before_resume"](123, {"pid": 123, "created": "100123"})
        self.operations.append(("start", argv, cwd))
        return 123

    def running_processes(self):
        self.polls += 1
        return [{"pid": 456, "created": "100456"}] if self.polls == 2 else []

    def exit_code(self):
        return 0

    def close(self):
        self.closed = True


def request(library, plan):
    session = library.root / "runtime" / "sessions" / plan.session_id
    session.mkdir(parents=True)
    path = session / "request.json"
    path.write_text(json.dumps(plan.record()), encoding="utf-8")
    from creation_lib.mod_manager.process_identity import current_identity
    helper_identity = current_identity()
    (session / "owner.json").write_text(json.dumps({"helper_pid": helper_identity["pid"],
        "helper_identity": helper_identity, "profile_id": plan.launch.profile_id}))
    return path


def test_documents_default_and_saved_isolation_share_runtime_policy(fixture):
    from creation_lib.mod_manager.profile_state import isolate_documents, fo4_runtime_ini
    from creation_lib.mod_manager.agent_launch import launch_options
    library, profile_id, executable, game, bundle, targets = fixture
    arguments = library, profile_id, executable, str(game), str(bundle)
    assert isolate_documents(library, profile_id) is False
    assert "SLocalSavePath = CustomSaves\\" in fo4_runtime_ini("Fallout4.ini", "[General]\nSLocalSavePath=CustomSaves\\\n", "")
    for isolated in (False, True, False):
        result = launch_options(library.root, profile_id, isolated)
        assert result["isolate_documents"] is isolated
        assert isolate_documents(Library(library.root), profile_id) is isolated
        plan = prepare(*arguments, user_targets=targets)
        assert plan.isolate_documents is isolated
        adapters = []
        def factory(*args):
            adapter = FakeVFS(*args)
            adapters.append(adapter)
            return adapter
        path = request(library, plan)
        assert run(path, native_factory=factory, wait=lambda _: None) == 0
        operations = adapters[0].operations
        mounted = [entry[2] for entry in operations if entry[0] == "directory"]
        assert (str(targets[1]) in mounted) is isolated
        assert (str(targets[1] / "F4SE") in mounted) is isolated
        assert (str(targets[1] / plan.save_subpath) in mounted) is isolated
        if not isolated:
            assert any(entry[0] == "file" and entry[2] == str(targets[1] / "Fallout4.ini") for entry in operations)
        status = json.loads((path.parent / "status.json").read_text())
        assert status["isolate_documents"] is isolated
        assert Path(status["documents_root"]) == (path.parent / "output/ini" if isolated else targets[1])
    isolate_documents(library, profile_id, True)
    assert not prepare(*arguments, user_targets=targets, isolate_documents=False).isolate_documents
    assert isolate_documents(library, profile_id) is True


@pytest.mark.skipif(os.name != "nt", reason="Windows profile lock API")
def test_helper_cancel_failure_child_lifetime_and_owned_outputs(fixture):
    library, profile_id, executable, game, bundle, targets = fixture
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    path = request(library, plan)
    adapters = []
    def factory(*args):
        result = FakeVFS(*args)
        adapters.append(result)
        return result
    assert run(path, native_factory=factory, wait=lambda _: None) == 0
    assert adapters[0].closed and adapters[0].polls == 4  # no early disconnect between parent and child
    status = json.loads((path.parent / "status.json").read_text())
    assert status["state"] == "completed" and status["exit_code"] == 0
    assert not any((game / "Data").iterdir())
    (path.parent / "cancel").touch()
    assert run(path, native_factory=lambda *args: pytest.fail("cancelled launch loaded DLL")) == 1
    assert json.loads((path.parent / "status.json").read_text())["state"] == "cancelled"
    (path.parent / "cancel").unlink()
    class Failed(FakeVFS):
        def start(self, argv, cwd, **kwargs):
            raise ManagerError("fixture injection failed")
    assert run(path, native_factory=Failed) == 1
    assert json.loads((path.parent / "status.json").read_text())["state"] == "failed"


def test_failed_supervision_retains_process_guard_until_asynchronous_termination(fixture, monkeypatch):
    from creation_lib.mod_manager.runtime import active_sessions
    library, profile_id, executable, game, bundle, targets = fixture
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    path = request(library, plan)
    class FailingSupervisor(FakeVFS):
        def running_processes(self):
            self.polls += 1
            if self.polls == 1:
                return [{"pid": 456, "created": "100456"}]
            raise ManagerError("fixture supervision failed")
    monkeypatch.setattr("creation_lib.mod_manager.process_identity.process_state", lambda identity: "alive" if identity["pid"] in {123, 456} else "exited")
    assert run(path, native_factory=FailingSupervisor, wait=lambda _: None) == 1
    status = json.loads((path.parent / "status.json").read_text())
    assert status["state"] == "supervision-failed" and status["pid"] == 123 and status["children"] == [456]
    assert (path.parent / "active.json").is_file()
    assert active_sessions(library.root)
    with pytest.raises(ManagerError, match="running native runtime"):
        Library(library.root).create_profile("Too early")
    monkeypatch.setattr("creation_lib.mod_manager.process_identity.process_state", lambda identity: "exited")
    assert active_sessions(library.root) == []
    assert Library(library.root).create_profile("After all children exit")


def test_stale_payload_and_profile_requests_fail_before_helper_spawn(fixture):
    library, profile_id, executable, game, bundle, targets = fixture
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    Path(plan.files[0][1]).write_bytes(b"changed fixture")
    with pytest.raises(ManagerError, match="changed after launch review"):
        start(library, plan, accept_source_writes=True, accept_minimal_ini=True, spawn=lambda *a, **k: pytest.fail("stale request started"))


def test_profile_signature_ignores_visual_group_but_tracks_payload_priority(fixture):
    library, profile_id, *_ = fixture
    row = next(row for row in library.rows(profile_id) if row.name == "Base")
    before = profile_signature(library, profile_id)
    library.customize(profile_id, row.id, group="Appearance/Textures", notes="Local edit")
    assert profile_signature(library, profile_id) == before
    library.reorder(profile_id, row.id, 10)
    assert profile_signature(library, profile_id) != before


def test_restart_running_session_blocks_metadata_and_unlocks_after_exit(fixture, monkeypatch):
    from creation_lib.mod_manager.controller import ManagerController
    library, profile_id, *_ = fixture
    path = library.root / "runtime" / "sessions" / "recovered"
    path.mkdir(parents=True)
    active_profile = library.root / "runtime" / "profiles" / profile_id
    active_profile.mkdir(parents=True)
    (active_profile / "active.json").write_text(json.dumps({"session": path.name}))
    (path / "status.json").write_text(json.dumps({"state": "running", "helper_pid": 1234,
        "helper_identity": {"pid": 1234, "created": "1001234"}, "pid": 5678,
        "process_identity": {"pid": 5678, "created": "1005678"}, "profile_id": profile_id}))
    monkeypatch.setattr("creation_lib.mod_manager.process_identity.process_state", lambda identity: "alive")
    reopened = Library(library.root)
    controller = ManagerController()
    try:
        controller.library = reopened
        controller.refresh()
        assert controller.runtime_active and controller.runtime_session == path
        with pytest.raises(ManagerError, match="running native runtime"):
            reopened.create_profile("Unsafe while running")
        with pytest.raises(ManagerError, match="running native runtime"):
            controller.create_profile("Unsafe while running")
        monkeypatch.setattr("creation_lib.mod_manager.process_identity.process_state", lambda identity: "exited")
        controller._runtime_poll_at = 0
        controller.poll_runtime()
        assert not controller.runtime_active and controller.runtime_status["state"] == "interrupted"
        assert reopened.create_profile("After exit")
    finally:
        controller.cleanup()


def test_ini_case_save_override_testfile_and_snapshot_refresh(fixture):
    library, profile_id, executable, game, bundle, targets = fixture
    profile = next(p for p in library.profiles() if p.id == profile_id)
    files = {**profile.files, "Fallout4.ini": "[general]\nslocalsavepath=Saves\\\n[Fixture]\nvalue=lowercase\n"}
    with library._db() as db:
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile_id))
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets, isolate_documents=True)
    ini = next(text for name, text, kind in plan.snapshots if name == "Fallout4.ini")
    assert "[general]" not in ini and "SLocalSavePath = __ModBox21Saves_" in ini and "Saves\\" not in ini
    path = request(library, plan)
    assert run(path, native_factory=FakeVFS, wait=lambda _: None) == 0
    files["Fallout4.ini"] = "[General]\n[Fixture]\nvalue=UPDATED\n"
    with library._db() as db:
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile_id))
    changed = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    path = request(library, changed)
    assert run(path, native_factory=FakeVFS, wait=lambda _: None) == 0
    assert "UPDATED" in (path.parent / "output" / "ini" / "Fallout4.ini").read_text()
    files["Fallout4.ini"] += "[general]\nsTestFile1=Untracked.esp\n"
    with library._db() as db:
        db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps(files), profile_id))
    with pytest.raises(ManagerError, match="Ambiguous|sTestFile"):
        prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)


def test_missing_plugin_snapshot_and_steam_fail_closed(fixture, monkeypatch):
    library, profile_id, executable, game, bundle, targets = fixture
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.steam_running", lambda: False)
    with pytest.raises(ManagerError, match="Start Steam"):
        start(library, plan, accept_source_writes=True, accept_minimal_ini=True,
              spawn=lambda *a, **k: pytest.fail("Steam-bootstrap fallback launched"))
    blank = library.create_profile("Blank profile")
    with pytest.raises(ManagerError, match="no plugin activation snapshot"):
        prepare(library, blank, executable, str(game), str(bundle), user_targets=targets)


def test_f4se_overrides_duplicates_and_late_config_are_rejected(fixture):
    library, profile_id, executable, game, bundle, targets = fixture
    with pytest.raises(ManagerError, match="overrides"):
        prepare(library, profile_id, replace(executable, arguments='-altexe "C:/Other/Fallout4.exe"'), str(game), str(bundle), user_targets=targets)
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    configuration = game / "Data" / "F4SE" / "f4se.ini"
    configuration.parent.mkdir()
    configuration.write_text("[Loader]\nRuntimeName=Other.exe\nRuntimeName=Fallout4.exe\n")
    with pytest.raises(ManagerError, match="Ambiguous"):
        prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    path = request(library, plan)
    assert run(path, native_factory=lambda *args: pytest.fail("ambiguous late config loaded DLL")) == 1
    assert "Ambiguous" in json.loads((path.parent / "status.json").read_text())["error"]
    configuration.write_text("[Loader]\nRuntimeName=Other.exe\n")
    assert run(path, native_factory=lambda *args: pytest.fail("late override loaded DLL")) == 1
    assert "RuntimeName" in json.loads((path.parent / "status.json").read_text())["error"]


def test_failed_parent_owner_write_stops_only_the_spawned_helper(fixture, monkeypatch):
    from types import SimpleNamespace
    library, profile_id, executable, game, bundle, targets = fixture
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.steam_running", lambda: True)
    events = []
    child = SimpleNamespace(pid=123, _handle=202, poll=lambda: None, terminate=lambda: events.append("terminate-own-helper"),
                            wait=lambda timeout: events.append("reap-own-helper"))
    original = Path.write_text
    def failed(path, *args, **kwargs):
        if path.name == "owner.tmp":
            raise OSError("fixture owner persistence failed")
        return original(path, *args, **kwargs)
    monkeypatch.setattr(Path, "write_text", failed)
    monkeypatch.setattr("creation_lib.mod_manager.process_identity.from_handle", lambda pid, handle: {"pid": pid, "created": "100123"})
    with pytest.raises(OSError, match="owner persistence"):
        start(library, plan, accept_source_writes=True, accept_minimal_ini=True, spawn=lambda *a, **k: child)
    assert events == ["terminate-own-helper", "reap-own-helper"]


def test_effective_owned_ini_cannot_restore_global_saves_or_bypass_plugins(fixture):
    library, profile_id, executable, game, bundle, targets = fixture
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets, isolate_documents=True)
    path = request(library, plan)
    assert run(path, native_factory=FakeVFS, wait=lambda _: None) == 0
    owned_ini = library.root / "runtime" / "profiles" / profile_id / "ini" / "Fallout4.ini"
    owned_ini.parent.mkdir(parents=True)
    baseline = owned_ini.parent.parent / "ini_baseline.json"
    baseline.write_bytes((path.parent / "output" / "ini_baseline.json").read_bytes())
    owned_ini.write_text("[general]\nslocalsavepath=Saves\\\n[Display]\nFixtureKeep=unchanged\n")
    plan = replace(plan, session_id=__import__("uuid").uuid4().hex)
    path = request(library, plan)
    assert run(path, native_factory=FakeVFS, wait=lambda _: None) == 0
    session_ini = path.parent / "output" / "ini" / "Fallout4.ini"
    assert plan.save_subpath in session_ini.read_text() and "FixtureKeep = unchanged" in session_ini.read_text()
    owned_ini.write_text("[General]\nSLocalSavePath=Saves\\\nsTestFile1=Bypass.esp\n")
    plan = replace(plan, session_id=__import__("uuid").uuid4().hex)
    path = request(library, plan)
    assert run(path, native_factory=lambda *args: pytest.fail("unsafe effective INI loaded DLL")) == 1
    assert "sTestFile" in json.loads((path.parent / "status.json").read_text())["error"]


@pytest.mark.parametrize("owner_error", ["creation", "profile", "extra-field"])
def test_helper_rejects_wrong_owner_before_native_construction(fixture, owner_error):
    library, profile_id, executable, game, bundle, targets = fixture
    path = request(library, prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets))
    owner_path = path.parent / "owner.json"
    owner = json.loads(owner_path.read_text())
    if owner_error == "creation":
        owner["helper_identity"]["created"] = str(int(owner["helper_identity"]["created"]) + 1)
    elif owner_error == "profile":
        owner["profile_id"] = "Other-profile"
    else:
        owner["helper_identity"]["unexpected"] = True
    owner_path.write_text(json.dumps(owner))
    assert run(path, native_factory=lambda *args: pytest.fail("unapproved helper loaded native code")) == 1
    status = json.loads((path.parent / "status.json").read_text())
    assert status["state"] == "failed" and "owner acknowledgement" in status["error"]
    assert not status.get("pid")


@pytest.mark.parametrize("ending", ["cancel", "timeout"])
def test_helper_missing_owner_cannot_create_process(fixture, monkeypatch, ending):
    library, profile_id, executable, game, bundle, targets = fixture
    path = request(library, prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets))
    (path.parent / "owner.json").unlink()
    if ending == "timeout":
        clock = iter([0, 31])
        monkeypatch.setattr("creation_lib.mod_manager.runtime_helper.time.monotonic", lambda: next(clock))
    assert run(path, native_factory=lambda *args: pytest.fail("ownerless helper loaded native code"),
               wait=lambda _: (path.parent / "cancel").touch()) == 1
    status = json.loads((path.parent / "status.json").read_text())
    assert status["state"] == ("cancelled" if ending == "cancel" else "failed") and not status.get("pid")


@pytest.mark.parametrize("ack_matches", [True, False])
def test_parent_ack_must_match_spawned_handle_creation_identity(fixture, monkeypatch, ack_matches):
    from types import SimpleNamespace
    from creation_lib.mod_manager import runtime
    library, profile_id, executable, game, bundle, targets = fixture
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    monkeypatch.setattr(runtime, "steam_running", lambda: True)
    events, identity = [], {"pid": 123, "created": "100123"}
    child = SimpleNamespace(pid=123, _handle=202, poll=lambda: None,
        terminate=lambda: events.append("terminate-retained-handle"), wait=lambda timeout: events.append("reaped"))
    monkeypatch.setattr("creation_lib.mod_manager.process_identity.from_handle", lambda pid, handle: identity)
    monkeypatch.setattr(runtime, "session_status", lambda session: {"state": "running", "helper_pid": 123,
        "helper_identity": identity if ack_matches else {"pid": 123, "created": "999999"}})
    if ack_matches:
        session, process, _ = start(library, plan, accept_source_writes=True, accept_minimal_ini=True, spawn=lambda *a, **k: child)
        assert process is child and json.loads((session / "owner.json").read_text())["helper_identity"] == identity
        assert not (session / "owner.tmp").exists() and not events
    else:
        with pytest.raises(ManagerError, match="creation identity"):
            start(library, plan, accept_source_writes=True, accept_minimal_ini=True, spawn=lambda *a, **k: child)
        assert events == ["terminate-retained-handle", "reaped"]


@pytest.mark.skipif(os.name != "nt", reason="Windows source-checkout helper")
def test_real_helper_waits_for_owner_and_exits_when_parent_identity_read_fails(fixture, monkeypatch):
    import subprocess
    import time
    from creation_lib.mod_manager import process_identity, runtime
    library, profile_id, executable, game, bundle, targets = fixture
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=targets)
    monkeypatch.setattr(runtime, "steam_running", lambda: True)
    captured, original = [], process_identity.from_handle
    def spawn(*args, **kwargs):
        kwargs["stderr"] = subprocess.PIPE
        child = subprocess.Popen(*args, **kwargs)
        captured.append((child, original(child.pid, child._handle), Path(args[0][-1]).parent))
        deadline = time.monotonic() + 8
        while child.poll() is None and time.monotonic() < deadline:
            if runtime.session_status(captured[0][2]).get("phase") == "awaiting-owner":
                return child
            time.sleep(.01)
        stderr = child.communicate(timeout=1)[1].decode(errors="replace") if child.poll() is not None else "still running"
        pytest.fail("Fixture helper did not wait for its owner: " + str(runtime.session_status(captured[0][2])) + stderr)
    def denied(*args, **kwargs):
        raise ManagerError("fixture parent creation-time query denied")
    monkeypatch.setattr(process_identity, "from_handle", denied)
    try:
        with pytest.raises(ManagerError, match="fixture parent creation-time query denied"):
            start(library, plan, accept_source_writes=True, accept_minimal_ini=True, spawn=spawn)
        child, _, session = captured[0]
        assert child.poll() is not None and not (session / "owner.json").exists()
        status = runtime.session_status(session)
        assert status["state"] == "cancelled" and not status.get("pid")
        assert runtime.active_sessions(library.root) == []
    finally:
        monkeypatch.setattr(process_identity, "from_handle", original)
        for child, identity, _ in captured:
            if child.poll() is None:
                process_identity.terminate_owned(child, identity)
            child.wait(timeout=10)


@pytest.mark.parametrize("persistent", [False, True])
def test_identity_status_atomic_publication_retries_only_bounded_sharing_errors(tmp_path, monkeypatch, persistent):
    from creation_lib.mod_manager.runtime_helper import write_status
    path = tmp_path / "status.json"
    path.write_text('{"state":"previous"}')
    original, calls = Path.replace, []
    def sharing(source, target):
        calls.append(source)
        if persistent or len(calls) <= 2:
            error = PermissionError("fixture Windows reader sharing conflict")
            error.winerror = 32
            raise error
        return original(source, target)
    monkeypatch.setattr(Path, "replace", sharing)
    monkeypatch.setattr("creation_lib.mod_manager.runtime_helper.time.sleep", lambda _: None)
    if persistent:
        with pytest.raises(PermissionError, match="sharing conflict"):
            write_status(path, state="running")
        assert len(calls) == 20 and json.loads(path.read_text())["state"] == "previous"
    else:
        write_status(path, state="running")
        assert len(calls) == 3 and json.loads(path.read_text())["helper_identity"]
