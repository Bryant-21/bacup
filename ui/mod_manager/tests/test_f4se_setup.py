"""Mock HTTP and generated PE/ZIP payloads only; no official binaries/game starts."""
from dataclasses import asdict
import io
import json
from pathlib import Path
import struct
import threading
import zipfile

import pytest

from creation_lib.mod_manager import f4se_setup, launch_targets, pe_metadata
from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import ManagerError
from creation_lib.mod_manager.pe_metadata import packed_runtime
from creation_lib.mod_manager.runtime import prepare, start
from creation_lib.mod_manager.runtime_helper import run
from .test_pe_metadata import fixture_version, image
from .test_runtime import FakeVFS, pe, request


def setup(tmp_path, monkeypatch, build=(1, 10, 163, 0)):
    monkeypatch.setattr(pe_metadata, "file_version", fixture_version)
    game = tmp_path / "game"
    (game / "Data").mkdir(parents=True)
    image(game / "Fallout4.exe", version=build)
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", game)
    identifier = launch_targets.apply(library, launch_targets.review(library, "Target", game))
    return library, profile, game, identifier


def package(tmp_path, build=(1, 10, 163, 0), *, version="", omit="", wrong_core=False):
    release = f4se_setup.CATALOG[build[:3]]
    folder = tmp_path / "package"
    folder.mkdir(exist_ok=True)
    loader_version = tuple(map(int, (version or release["version"]).split("."))) + (0,)
    image(folder / "f4se_loader.exe", version=loader_version)
    image(folder / f4se_setup.runtime_name(build), "F4SECore_Version", struct.pack("<II", 1, packed_runtime((1, 1, 1) if wrong_core else build)))
    if release["steam_loader"]:
        image(folder / "f4se_steam_loader.dll")
    (folder / "Data" / "Scripts").mkdir(parents=True, exist_ok=True)
    (folder / "Data" / "Scripts" / "F4SE.pex").write_bytes(b"inert generated script")
    (folder / "src").mkdir(exist_ok=True)
    (folder / "src" / "not-runtime.cpp").write_text("generated ignored source")
    archive = tmp_path / "fixture.zip"
    with zipfile.ZipFile(archive, "w") as writer:
        for path in folder.rglob("*"):
            if path.is_file() and path.name != omit:
                writer.write(path, "f4se_fixture/" + path.relative_to(folder).as_posix())
    return archive


@pytest.mark.parametrize("build", [(1, 10, 163, 0), (1, 10, 984, 0), (1, 11, 240, 9)])
def test_private_stage_exact_release_scripts_reopen_preserves_existing(tmp_path, monkeypatch, build):
    library, profile, game, identifier = setup(tmp_path, monkeypatch, build)
    image(game / "f4se_loader.exe", version=(0, 1, 0, 0))
    (game / "Data" / "custom.txt").write_bytes(b"custom preserved")
    original = {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()}
    value = f4se_setup.prepare(library, identifier, archive=package(tmp_path, build))
    assert value.f4se["managed"] and value.f4se["state"] == "ready"
    assert f4se_setup.validate_ready(value)["build"] == list(build)
    assert not (Path(value.f4se["stage"]) / "src").exists()
    view = launch_targets.TargetLibrary(Library(library.root), identifier)
    assert view.profile_snapshot(profile)[1][0].source == value.f4se["data"]
    assert {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()} == original
    names = {item[0] for item in value.f4se["root_files"]}
    assert ("f4se_steam_loader.dll" in names) == (build[:3] == (1, 10, 163))
    repeated = f4se_setup.prepare(library, identifier, archive=tmp_path / "fixture.zip")
    assert repeated.f4se["manifest"] == value.f4se["manifest"]
    assert Path(value.f4se["stage"]).is_dir()  # retain prior complete version for recovery
    assert not list((view.root / "f4se").glob("pending-*"))


@pytest.mark.parametrize("fault", ["loader", "dll", "scripts", "steam", "version", "core", "cancel", "failure", "changed-game", "changed-target"])
def test_failed_or_cancelled_stage_does_not_publish_or_touch_game(tmp_path, monkeypatch, fault):
    library, _, game, identifier = setup(tmp_path, monkeypatch)
    before = asdict(launch_targets.records(library)[0])
    omitted = {"loader": "f4se_loader.exe", "dll": "f4se_1_10_163.dll", "scripts": "F4SE.pex", "steam": "f4se_steam_loader.dll"}.get(fault, "")
    archive = package(tmp_path, omit=omitted, version="0.7.9" if fault == "version" else "", wrong_core=fault == "core")
    cancel = threading.Event()
    if fault == "cancel":
        cancel.set()
    if fault == "failure":
        monkeypatch.setattr(f4se_setup, "_checkpoint", lambda: (_ for _ in ()).throw(RuntimeError("fixture fault")))
    elif fault == "changed-game":
        monkeypatch.setattr(f4se_setup, "_checkpoint", lambda: image(game / "Fallout4.exe", version=(1, 11, 240, 0)))
    elif fault == "changed-target":
        # External configuration change at the publication boundary is observed
        # by a second review before preparation; this tests the frozen identity.
        original_image = pe_metadata.file_version
        count = [0]
        def version(path):
            count[0] += 1
            if count[0] == 2:
                with library._db() as db:
                    record = json.loads(db.execute("SELECT record FROM launch_targets WHERE id=?", (identifier,)).fetchone()[0])
                    record["name"] = "External edit"
                    db.execute("UPDATE launch_targets SET name=?,record=? WHERE id=?", (record["name"], json.dumps(record), identifier))
            return original_image(path)
        monkeypatch.setattr(pe_metadata, "file_version", version)
    with pytest.raises((ManagerError, RuntimeError)):
        f4se_setup.prepare(library, identifier, archive=archive, cancel=cancel)
    after = launch_targets.records(library)[0]
    assert after.f4se == before["f4se"]
    state = library.root / ".launch-targets" / identifier / "f4se"
    assert not state.exists() or list(state.iterdir()) == []
    assert not (game / "f4se_loader.exe").exists()


@pytest.mark.parametrize("marker", ["MicrosoftGame.config", "goggame-fixture.info", ".egstore"])
def test_storefront_unsupported_and_unknown_build(tmp_path, monkeypatch, marker):
    library, _, game, identifier = setup(tmp_path, monkeypatch)
    (game / marker).write_text("generated store marker")
    assert f4se_setup.inspect_folder(game)["state"] == "unsupported"
    with pytest.raises(ManagerError, match="adapter"):
        f4se_setup.prepare(library, identifier)
    (game / marker).unlink()
    image(game / "Fallout4.exe", version=(1, 12, 999, 0))
    assert f4se_setup.inspect_folder(game)["state"] == "unsupported"
    image(game / "Fallout4.exe")
    assert f4se_setup.inspect_folder(game)["state"] == "unknown"


def test_installed_matching_and_custom_mismatch_preserved(tmp_path, monkeypatch):
    _, _, game, _ = setup(tmp_path, monkeypatch)
    archive = package(tmp_path)
    with zipfile.ZipFile(archive) as reader:
        for name in reader.namelist():
            if "/src/" not in name:
                path = game / name.split("/", 1)[1]
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(reader.read(name))
    assert f4se_setup.inspect_folder(game)["state"] == "ready"
    image(game / "f4se_loader.exe", version=(0, 7, 9, 0))
    before = (game / "f4se_loader.exe").read_bytes()
    assert f4se_setup.inspect_folder(game)["state"] == "mismatch"
    assert (game / "f4se_loader.exe").read_bytes() == before


def test_bounded_official_download_cancel_short_bad_redirect_and_nexus_identity(tmp_path, monkeypatch):
    library, _, _, identifier = setup(tmp_path, monkeypatch)
    class Response(io.BytesIO):
        headers = {"Content-Length": "100"}
        def geturl(self):
            return "https://f4se.silverlock.org/beta/fixture.7z"
    with pytest.raises(ManagerError, match="incomplete"):
        f4se_setup.prepare(library, identifier, opener=lambda _: Response(b"short"))
    with pytest.raises(ManagerError, match="HTTPS"):
        f4se_setup._download("http://f4se.silverlock.org/unsafe", tmp_path / "unsafe.zip", None, None)
    class Redirect(Response):
        def geturl(self):
            return "https://untrusted.example/fixture.7z"
    with pytest.raises(ManagerError, match="HTTPS"):
        f4se_setup.prepare(library, identifier, opener=lambda _: Redirect(b"data"))
    image(Path(launch_targets.records(library)[0].game_root) / "Fallout4.exe", version=(1, 11, 240, 0))
    with pytest.raises(ManagerError, match="session account"):
        f4se_setup.prepare(library, identifier)
    with pytest.raises(ManagerError, match="exact official"):
        f4se_setup.prepare(library, identifier, client=object(), nxm="nxm://fallout4/mods/42147/files/1")


def test_managed_runtime_plan_helper_root_mapping_and_experimental_gate(tmp_path, monkeypatch):
    library, profile, game, identifier = setup(tmp_path, monkeypatch)
    f4se_setup.prepare(library, identifier, archive=package(tmp_path))
    view = launch_targets.TargetLibrary(library, identifier)
    bundle = tmp_path / "bundle"
    bundle.mkdir()
    for name, machine in (("usvfs_x64.dll", 0x8664), ("usvfs_x86.dll", 0x14c), ("usvfs_proxy_x64.exe", 0x8664), ("usvfs_proxy_x86.exe", 0x14c)):
        pe(bundle / name, machine)
    user = (tmp_path / "global plugins", tmp_path / "global INIs")
    value = view.target()
    plan = prepare(view, profile, value.executable, game, bundle, user_targets=user, isolate_documents=True)
    assert plan.experimental and plan.metadata_root == str(library.root) and plan.target_id == identifier
    assert "-altexe" in plan.launch.argv and "-altdll" in plan.launch.argv
    assert any(name == "Scripts/F4SE.pex" for name, *_ in plan.files)
    assert identifier in plan.save_subpath
    with pytest.raises(ManagerError, match="experimental"):
        start(view, plan, accept_source_writes=True, accept_minimal_ini=True)
    adapters = []
    def factory(*args):
        adapters.append(FakeVFS(*args))
        return adapters[-1]
    assert run(request(view, plan), native_factory=factory, wait=lambda _: None) == 0
    mappings = [item for item in adapters[0].operations if item[0] == "file"]
    assert any(item[2] == str(game / "f4se_1_10_163.dll") for item in mappings)
    assert not (game / "f4se_1_10_163.dll").exists()
    assert any(item[2] == str(user[1] / "F4SE") for item in adapters[0].operations if item[0] == "directory")
    (Path(value.f4se["data"]) / "Scripts" / "F4SE.pex").write_bytes(b"changed")
    with pytest.raises(ManagerError, match="changed"):
        f4se_setup.validate_ready(value)


@pytest.mark.parametrize("mode", ["exact", "mismatch", "flags", "unknown", "new-se", "x86"])
def test_static_native_plugin_compatibility_separate_from_provisioning(tmp_path, mode):
    raw = bytearray(604)
    struct.pack_into("<I", raw, 0, 1)
    struct.pack_into("<I", raw, 528, packed_runtime((1, 10, 163) if mode != "mismatch" else (1, 11, 240)))
    if mode == "flags":
        struct.pack_into("<I", raw, 520, 1)
    if mode == "new-se":
        struct.pack_into("<I", raw, 592, packed_runtime((0, 7, 9)))
    path = image(tmp_path / "inert.dll", "" if mode == "unknown" else "F4SEPlugin_Version", raw)
    if mode == "x86":
        changed = bytearray(path.read_bytes())
        struct.pack_into("<H", changed, 0x104, 0x14c)
        path.write_bytes(changed)
    files = (("F4SE/Plugins/fixture.dll", str(path), ()),)
    if mode in {"mismatch", "new-se", "x86"}:
        with pytest.raises(ManagerError):
            f4se_setup.plugin_findings(files, (1, 10, 163), "0.6.23")
    else:
        findings = f4se_setup.plugin_findings(files, (1, 10, 163), "0.6.23")
        assert len(findings) == 1
        assert ("unknown" if mode == "unknown" else "unverified") in findings[0]


@pytest.mark.parametrize("mode", ["premium", "free", "denied", "bad-md5"])
def test_exact_nexus_acquisition_mock_entitlement_private_credentials_and_checksum(tmp_path, monkeypatch, mode):
    import hashlib
    library, _, game, identifier = setup(tmp_path, monkeypatch, (1, 11, 240, 0))
    payload = package(tmp_path, (1, 11, 240, 0)).read_bytes()
    class Response(io.BytesIO):
        headers = {"Content-Length": str(len(payload))}
        def geturl(self):
            return "https://files.nexus-cdn.com/fixture.zip"
    class Client:
        forgotten = False
        def for_transfer(self):
            return self
        def resolve(self, link):
            assert link.identity == ("fallout4", 42147, 407709)
            if mode == "free":
                assert link.key == "fixture-secret" and link.user_id == 42
            if mode == "denied":
                raise ManagerError("Mock entitlement denied")
            return {"file_name": "fixture.zip", "md5": "0" * 32 if mode == "bad-md5" else hashlib.md5(payload, usedforsecurity=False).hexdigest()}, "https://files.nexus-cdn.com/fixture.zip"
        def _open(self, request, api=True):
            assert not api and "apikey" not in {name.casefold() for name in request.headers}
            return Response(payload)
        def forget(self):
            self.forgotten = True
    client = Client()
    nxm = "nxm://fallout4/mods/42147/files/407709?key=fixture-secret&expires=4000000000&user_id=42" if mode == "free" else ""
    if mode in {"denied", "bad-md5"}:
        with pytest.raises(ManagerError):
            f4se_setup.prepare(library, identifier, client=client, nxm=nxm)
        assert launch_targets.records(library)[0].f4se["state"] != "ready"
    else:
        result = f4se_setup.prepare(library, identifier, client=client, nxm=nxm)
        assert result.f4se["state"] == "ready"
    assert client.forgotten and not (game / "f4se_loader.exe").exists()
    assert "fixture-secret" not in json.dumps(asdict(launch_targets.records(library)[0]))


def test_review_changed_managed_game_replaces_old_receipt_with_unready_build(tmp_path, monkeypatch):
    library, _, game, identifier = setup(tmp_path, monkeypatch)
    old = f4se_setup.prepare(library, identifier, archive=package(tmp_path))
    image(game / "Fallout4.exe", version=(1, 11, 240, 0))
    plan = launch_targets.review(library, "Target", game, identifier=identifier)
    assert plan.target.f4se["state"] == "missing"
    assert plan.target.f4se["build"][:3] == [1, 11, 240]
    launch_targets.apply(library, plan)
    assert Path(old.f4se["stage"]).is_dir()
    with pytest.raises(ManagerError, match="not been prepared"):
        f4se_setup.validate_ready(launch_targets.records(library)[0])


def test_native_plugin_flag_generation_missing_address_library_and_breaking_reserve(tmp_path):
    raw = bytearray(604)
    struct.pack_into("<I", raw, 0, 1)
    struct.pack_into("<II", raw, 520, 4, 4)
    plugin = image(tmp_path / "inert.dll", "F4SEPlugin_Version", raw)
    files = [("F4SE/Plugins/fixture.dll", str(plugin), ())]
    with pytest.raises(ManagerError, match="Address Library"):
        f4se_setup.plugin_findings(files, (1, 11, 240), "0.7.9")
    files.append(("F4SE/Plugins/version-1-11-240-0.bin", str(tmp_path / "inert-address.bin"), ()))
    assert "unverified" in f4se_setup.plugin_findings(files, (1, 11, 240), "0.7.9")[0]
    with pytest.raises(ManagerError, match="runtime list"):
        f4se_setup.plugin_findings(files, (1, 10, 984), "0.7.2")
    struct.pack_into("<I", raw, 600, 1)
    image(plugin, "F4SEPlugin_Version", raw)
    with pytest.raises(ManagerError, match="runtime list"):
        f4se_setup.plugin_findings(files, (1, 11, 240), "0.7.9")
