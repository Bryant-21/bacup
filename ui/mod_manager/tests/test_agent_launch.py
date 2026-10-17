"""Headless agent entry: MO2 mirror, launch-time INI scope, detached spawn and PID-scoped stop."""
import ctypes as ct
import json
import os
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

import pytest

from creation_lib.mod_manager import agent_launch
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import ManagerError
from creation_lib.mod_manager.runtime_helper import run
from .fixtures import make_mo2
from .test_runtime import FakeVFS, pe


def tree_bytes(root):
    return {str(path.relative_to(root)): path.read_bytes() for path in Path(root).rglob("*") if path.is_file()}


@pytest.fixture
def instance(tmp_path):
    source = make_mo2(tmp_path)
    game = tmp_path / "game"
    (game / "Data").mkdir(parents=True)
    pe(game / "f4se_loader.exe")
    pe(game / "Fallout4.exe")
    other = tmp_path / "other-game"
    other.mkdir()
    pe(other / "f4se_loader.exe")
    ini = source / "ModOrganizer.ini"
    ini.write_text(ini.read_text("utf-8").replace("[General]\n", "[General]\ngamePath=@ByteArray(" + str(game).replace("\\", "\\\\") + ")\nselected_profile=@ByteArray(Default)\n")
                   + "[customExecutables]\nsize=2\n"
                   + f"1\\title=F4SE\n1\\binary={(game / 'f4se_loader.exe').as_posix()}\n1\\arguments=-forcesteamloader\n1\\workingDirectory={game.as_posix()}\n"
                   + f"2\\title=F4SE OG\n2\\binary={(other / 'f4se_loader.exe').as_posix()}\n2\\workingDirectory={other.as_posix()}\n", encoding="utf-8")
    plugin_ini = source / "mods" / "Override" / "F4SE" / "Plugins" / "HighFPSPhysicsFix.ini"
    plugin_ini.parent.mkdir(parents=True)
    plugin_ini.write_text("; fixture comment\n[Main]\nEnableVSync=false\n\n[Window]\n; keep me\nLockCursor=true\nBorderless=false\n", encoding="utf-8")
    user = tmp_path / "game-user"
    (user / "plugins").mkdir(parents=True)
    (user / "ini").mkdir()
    (user / "ini" / "Fallout4.INI").write_text("[General]\nsGlobalMarker=documents\n[Archive]\nsResourceDataDirsFinal=STRINGS\\\n", encoding="utf-8")
    (user / "ini" / "Fallout4Prefs.ini").write_text("[Display]\nbBorderless=1\nbFull Screen=1\niSize W=3840\niSize H=2160\n", encoding="utf-8")
    (user / "ini" / "Fallout4Custom.ini").write_text("[Display]\niLocation X=0\n", encoding="utf-8")
    library_root = tmp_path / "agent-library"
    return SimpleNamespace(source=source, game=game, library=library_root, targets=(user / "plugins", user / "ini"),
                           user=user, tmp=tmp_path)


def synced(instance):
    result = agent_launch.sync(instance.source, instance.library)
    return result, Library(instance.library)


def captured_launch(instance, monkeypatch, **options):
    calls = []
    def fake_start(library, plan, **kwargs):
        calls.append((plan, kwargs))
        session = library.root / "runtime" / "sessions" / plan.session_id
        session.mkdir(parents=True, exist_ok=True)
        return session, SimpleNamespace(pid=11), {"state": "running", "helper_pid": 11, "pid": 123,
                                                  "process_identity": {"pid": 123, "created": "100123"}}
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.ensure", lambda cancel, progress: instance.tmp / "bundle")
    monkeypatch.setattr("creation_lib.mod_manager.runtime.start", fake_start)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.process_names", lambda: {"steam.exe"})
    options.setdefault("user_targets", instance.targets)
    result = agent_launch.launch(instance.library, "Default", "f4se", **options)
    return result, calls[0][0], calls[0][1]


def test_cli_uses_selected_loader_directory_instead_of_imported_game_path(instance, monkeypatch):
    synced(instance)
    other = instance.tmp / "other-game"
    (other / "Data").mkdir()
    pe(other / "Fallout4.exe")
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.ensure", lambda *args: instance.tmp / "bundle")
    result = agent_launch.launch(instance.library, "Default", "F4SE OG", dry_run=True, user_targets=instance.targets)
    assert result["game_root"] == str(other)
    assert result["executable"]["argv"][0] == str(other / "f4se_loader.exe")


def test_launch_allows_another_game_without_touching_existing_processes(instance, monkeypatch):
    synced(instance)
    result, plan, _ = captured_launch(instance, monkeypatch)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.process_names", lambda: {"steam.exe", "fallout4.exe"})
    second = agent_launch.launch(instance.library, "Default", "f4se", user_targets=instance.targets)
    assert second["state"] == "running"
    assert second["output"]["overwrite"] != result["output"]["overwrite"]



def helper_run(library, plan, monkeypatch):
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.validate", lambda root, cancel=None: Path(root))
    session = library.root / "runtime" / "sessions" / plan.session_id
    session.mkdir(parents=True, exist_ok=True)
    path = session / "request.json"
    path.write_text(json.dumps(plan.record()), encoding="utf-8")
    from creation_lib.mod_manager.process_identity import current_identity
    identity = current_identity()
    (session / "owner.json").write_text(json.dumps({"helper_pid": identity["pid"], "helper_identity": identity,
                                                    "profile_id": plan.launch.profile_id}))
    class ProjectFake(FakeVFS):
        def configure(self, owner, config, protected=()):
            self.operations.append(("configure", str(owner)))
    adapters = []
    def factory(*args):
        adapters.append(ProjectFake(*args))
        return adapters[-1]
    assert run(path, native_factory=factory, wait=lambda _: None) == 0, (session / "status.json").read_text()
    return adapters[0]


def test_sync_mirrors_current_mo2_profile_by_reference_and_never_writes_source(instance):
    before = tree_bytes(instance.source)
    result, library = synced(instance)
    assert tree_bytes(instance.source) == before
    default = next(profile for profile in result["profiles"] if profile["name"] == "Default")
    rows = {row.name: row for row in library.rows(default["id"])}
    assert rows["Override"].enabled and rows["Base"].enabled and not rows["Disabled"].enabled
    assert rows["Override"].priority > rows["Base"].priority
    assert rows["Override"].source == str(instance.source / "mods" / "Override")
    assert not list(instance.library.rglob("shared.txt"))  # no payload copies
    assert result["game_root"] == str(instance.game)
    # MO2 remains the source of truth: later toggles/order/plugins are mirrored on the next sync.
    profile_dir = instance.source / "profiles" / "Default"
    (profile_dir / "modlist.txt").write_text("+Disabled\n+Base\n-Override\n-Visual_separator\n", encoding="utf-8")
    (profile_dir / "plugins.txt").write_text("Fixture.esp\n*Disabled.esp\n", encoding="utf-8")
    before = tree_bytes(instance.source)
    agent_launch.sync(instance.source, instance.library)
    assert tree_bytes(instance.source) == before
    rows = {row.name: row for row in Library(instance.library).rows(default["id"])}
    assert rows["Disabled"].enabled and rows["Base"].enabled and not rows["Override"].enabled
    assert rows["Disabled"].priority > rows["Base"].priority
    profile = next(p for p in Library(instance.library).profiles() if p.id == default["id"])
    assert profile.files["plugins.txt"].splitlines() == ["Fixture.esp", "*Disabled.esp"]
    assert profile.files["_modbox_instance_id"]


def test_sync_refuses_library_inside_mo2_or_game(instance):
    with pytest.raises(ManagerError, match="outside"):
        agent_launch.sync(instance.source, instance.source / "agent")
    assert not (instance.source / "agent").exists()


def test_launch_selects_instance_f4se_and_reports_mappings_and_output(instance, monkeypatch):
    synced(instance)
    result, plan, options = captured_launch(instance, monkeypatch, accept_minimal_ini=True)
    assert Path(plan.launch.argv[0]) == instance.game / "f4se_loader.exe" and "-forcesteamloader" in plan.launch.argv
    assert options["accept_minimal_ini"] and options["spawn"] is agent_launch.detached_spawn
    assert result["pid"] == 123 and result["helper_pid"] == 11 and result["state"] == "running"
    profile_root = instance.library / "runtime" / "sessions" / plan.session_id / "output"
    assert result["output"]["overwrite"] == str(profile_root / "overwrite")
    assert result["output"]["saves"] == str(instance.targets[1] / "Saves")
    assert result["output"]["f4se_logs"] == str(instance.targets[1] / "F4SE")
    assert result["isolate_documents"] is False
    mappings = json.loads(Path(result["mappings"]).read_text("utf-8"))
    assert result["mapped_files"] == len(mappings["files"]) == len(plan.files)
    data = {entry["path"].casefold(): entry["source"] for entry in mappings["files"]}
    assert data["data/textures/shared.txt"] == str(instance.source / "mods" / "Override" / "textures" / "shared.txt")
    assert mappings["mounts"][str(instance.game / "Data")] == str(profile_root / "overwrite")
    assert str(instance.targets[1]) not in mappings["mounts"]


def test_launch_rejects_ambiguous_or_unknown_executable_and_profile(instance, monkeypatch):
    synced(instance)
    monkeypatch.setattr("creation_lib.mod_manager.runtime.start", lambda *a, **k: pytest.fail("started"))
    with pytest.raises(ManagerError, match="No executable"):
        agent_launch.launch(instance.library, "Default", "Missing tool", user_targets=instance.targets)
    with pytest.raises(ManagerError, match="No profile"):
        agent_launch.launch(instance.library, "Nope", "f4se", user_targets=instance.targets)


def test_auto_ini_source_follows_mo2_local_settings_and_reads_global_inis_read_only(instance, monkeypatch):
    synced(instance)
    before = tree_bytes(instance.user)
    result, plan, _ = captured_launch(instance, monkeypatch)
    inis = {name.casefold(): text for name, text, kind in plan.snapshots if kind == "ini"}
    assert "sGlobalMarker" in inis["fallout4.ini"] and "value = unchanged" not in inis["fallout4.ini"]
    assert "SLocalSavePath = Saves\\" in inis["fallout4.ini"]
    assert result["ini_sources"] == {"Fallout4.ini": "global", "Fallout4Prefs.ini": "global", "Fallout4Custom.ini": "global"}
    assert plan.minimal_ini == ()
    assert tree_bytes(instance.user) == before
    profile = instance.source / "profiles" / "Default"
    (profile / "settings.ini").write_text("[General]\nLocalSettings=true\n", encoding="utf-8")
    synced(instance)
    monkeypatch.undo()
    result, plan, _ = captured_launch(instance, monkeypatch, accept_minimal_ini=True)
    inis = {name.casefold(): text for name, text, kind in plan.snapshots if kind == "ini"}
    assert "value = unchanged" in inis["fallout4.ini"] and "sGlobalMarker" not in inis["fallout4.ini"]
    assert result["ini_sources"]["Fallout4.ini"] == "profile" and result["ini_sources"]["Fallout4Prefs.ini"] == "minimal"


def test_agent_defaults_apply_only_to_owned_runtime_copies_and_are_restored(instance, monkeypatch):
    synced(instance)
    source_before, user_before = tree_bytes(instance.source), tree_bytes(instance.user)
    library = Library(instance.library)
    profile_before = {p.id: p.files for p in library.profiles()}
    result, plan, _ = captured_launch(instance, monkeypatch, agent_defaults=True)
    inis = {name.casefold(): text for name, text, kind in plan.snapshots if kind == "ini"}
    assert "bAlwaysActive=1" in inis["fallout4custom.ini"] and "bBackgroundMouse=1" in inis["fallout4custom.ini"]
    assert "bFull Screen=0" in inis["fallout4prefs.ini"] and "iSize W=1920" in inis["fallout4prefs.ini"] and "iSize H=1080" in inis["fallout4prefs.ini"]
    assert "3840" not in inis["fallout4prefs.ini"] and "bBorderless = 1" in inis["fallout4prefs.ini"]
    files = {name.casefold(): source for name, source, _ in plan.files}
    generated = Path(files["f4se/plugins/highfpsphysicsfix.ini"])
    assert generated.is_relative_to(instance.library / "runtime")
    text = generated.read_text("utf-8")
    assert "LockCursor=false" in text and "LockCursor=true" not in text
    assert "; keep me" in text and "; fixture comment" in text and "Borderless=false" in text
    assert {"file": "F4SE/Plugins/HighFPSPhysicsFix.ini", "section": "Window", "key": "LockCursor", "value": "false",
            "scope": "data"} in result["overrides"]
    assert result["skipped_overrides"] == [{"file": "F4SE/Plugins/HighFPSPhysicsFix_custom.ini", "reason": "not present in mods or game Data (paths are relative to Data)"}]
    assert tree_bytes(instance.source) == source_before and tree_bytes(instance.user) == user_before
    assert {p.id: p.files for p in Library(instance.library).profiles()} == profile_before
    adapter = helper_run(library, plan, monkeypatch)
    owned = instance.library / "runtime" / "sessions" / plan.session_id / "output" / "ini"
    assert "bAlwaysActive=1" in (owned / "Fallout4Custom.ini").read_text().replace(" = ", "=")
    data = Path(plan.game_root) / "Data"
    assert ("file", str(generated), str(data / "F4SE/Plugins/HighFPSPhysicsFix.ini")) in adapter.operations
    # A later launch without overrides re-derives the owned copies from the unchanged profile scope.
    monkeypatch.undo()
    _, plain, _ = captured_launch(instance, monkeypatch)
    helper_run(library, plain, monkeypatch)
    owned = instance.library / "runtime" / "sessions" / plain.session_id / "output" / "ini"
    assert "bAlwaysActive" not in (owned / "Fallout4Custom.ini").read_text()
    assert "iSize W=3840" in (owned / "Fallout4Prefs.ini").read_text().replace(" = ", "=")
    files = {name.casefold(): source for name, source, _ in plain.files}
    assert files["f4se/plugins/highfpsphysicsfix.ini"] == str(instance.source / "mods" / "Override" / "F4SE" / "Plugins" / "HighFPSPhysicsFix.ini")
    assert tree_bytes(instance.source) == source_before and tree_bytes(instance.user) == user_before


def test_data_override_uses_game_data_base_skips_absent_and_refuses_owned_shadow(instance, monkeypatch):
    synced(instance)
    (instance.game / "Data" / "Game.ini").write_text("[A]\nx=1\n", encoding="utf-8")
    game_before = tree_bytes(instance.game)
    result, plan, _ = captured_launch(instance, monkeypatch, overrides=("Game.ini:A:x=2", "Absent.ini:A:x=2"))
    files = {name.casefold(): source for name, source, _ in plan.files}
    assert "x=2" in Path(files["game.ini"]).read_text("utf-8") and tree_bytes(instance.game) == game_before
    assert result["skipped_overrides"] == [{"file": "Absent.ini", "reason": "not present in mods or game Data (paths are relative to Data)"}]
    shadow = instance.library / "runtime" / "profiles" / plan.launch.profile_id / "overwrite" / "Game.ini"
    shadow.parent.mkdir(parents=True, exist_ok=True)
    shadow.write_text("[A]\nx=9\n", encoding="utf-8")
    with pytest.raises(ManagerError, match="overwrite"):
        captured_launch(instance, monkeypatch, overrides=("Game.ini:A:x=2",))


@pytest.mark.parametrize("spec", ["Fallout4.ini:General:sLocalSavePath=Saves\\", "Fallout4.ini:General:sTestFile1=X.esp",
                                  "Fallout4.ini:General", "../Escape.ini:A:x=1", "Fallout4.ini::x=1"])
def test_invalid_or_isolation_breaking_overrides_are_refused(spec):
    with pytest.raises(ManagerError):
        agent_launch.parse_override(spec)


def test_set_ini_values_replaces_every_duplicate_and_appends_missing_sections():
    text = "[Window]\r\nLockCursor=true\r\nlockcursor = true\r\n[Other]\r\nx=1"
    result = agent_launch.set_ini_values(text, {"window": ("Window", {"LockCursor": "false", "New": "1"}), "added": ("Added", {"k": "v"})})
    assert result.count("LockCursor=false") == 1 and result.count("lockcursor=false") == 1 and "true" not in result
    assert "New=1\r\n[Other]" in result and result.endswith("[Added]\r\nk=v\r\n") and "\r\n\n" not in result


def test_detached_spawn_hides_console_and_breaks_away_from_enclosing_job_when_allowed():
    calls = []
    def popen(args, **kwargs):
        calls.append(kwargs["creationflags"])
        if len(calls) == 1 and denied:
            raise PermissionError(13, "Access is denied")
        return "child"
    for denied in (False, True):
        calls.clear()
        assert agent_launch.detached_spawn(["helper"], creationflags=subprocess.CREATE_NO_WINDOW, popen=popen) == "child"
        assert calls[0] == 0x08000000 | 0x01000000
        if denied:
            assert calls[1] == 0x08000000


@pytest.mark.skipif(os.name != "nt" or ct.sizeof(ct.c_void_p) != 8, reason="x64 Windows ABI")
def test_project_filespace_creates_console_loader_without_a_console_window(tmp_path):
    from creation_lib.mod_manager.native_runtime import ProcessInfo
    from creation_lib.mod_manager.project_vfs import NativeFilespace
    flags = []
    def create(exe, command, pattrs, tattrs, inherit, value, env, cwd, startup, info):
        flags.append(value)
        out = ct.cast(info, ct.POINTER(ProcessInfo)).contents
        out.process, out.thread, out.pid = 202, 303, 404
        return True
    def initialize(buffer, count, flag, size):
        ct.cast(size, ct.POINTER(ct.c_size_t)).contents.value = 64
        return True
    def times(handle, created, *args):
        ct.cast(created, ct.POINTER(ct.wintypes.FILETIME)).contents.dwLowDateTime = 100404
        return True
    adapter = NativeFilespace.__new__(NativeFilespace)
    adapter.owner, adapter.config, adapter.session = tmp_path, tmp_path / "filespace.txt", "fixture"
    adapter.files, adapter.mounts, adapter.protected = {}, {}, set()
    adapter.library = SimpleNamespace(MBVFS_Inject=lambda handle, config: 1)
    adapter.kernel = SimpleNamespace(CreateJobObjectW=lambda *a: 101, SetInformationJobObject=lambda *a: True,
                                     InitializeProcThreadAttributeList=initialize, UpdateProcThreadAttribute=lambda *a: True,
                                     DeleteProcThreadAttributeList=lambda *a: None, CreateProcessW=create,
                                     GetProcessId=lambda handle: 404, GetProcessTimes=times,
                                     ResumeThread=lambda thread: 0, CloseHandle=lambda handle: True)
    assert adapter.start(["f4se_loader.exe"], str(tmp_path)) == 404
    assert flags[0] & 0x08000000 and flags[0] & 4 and flags[0] & 0x80000
    assert not flags[0] & 0x00000010 and not flags[0] & 0x00000008  # no new console, not detached from job hooks


def write_session(library_root, profile_id, process, identity, *, name="fixture-session", children=()):
    from creation_lib.mod_manager.process_identity import from_handle
    session = Path(library_root) / "runtime" / "sessions" / name
    session.mkdir(parents=True)
    child_identities = [from_handle(child.pid, child._handle) for child in children]
    (session / "status.json").write_text(json.dumps({"state": "running", "helper_pid": process.pid, "helper_identity": identity,
        "pid": process.pid, "process_identity": identity, "children": [child.pid for child in children],
        "child_identities": child_identities, "profile_id": profile_id}))
    active = Path(library_root) / "runtime" / "profiles" / profile_id
    active.mkdir(parents=True, exist_ok=True)
    (active / "active.json").write_text(json.dumps({"session": name}))
    return session


@pytest.mark.skipif(os.name != "nt", reason="Windows process identities")
def test_stop_terminates_only_the_verified_session_owner_by_recorded_pid(tmp_path):
    from creation_lib.mod_manager.process_identity import from_handle
    library = Library(tmp_path / "library", create=True)
    sleeper = [sys.executable, "-c", "import time; time.sleep(60)"]
    owner = subprocess.Popen(sleeper, creationflags=subprocess.CREATE_NO_WINDOW)
    game = subprocess.Popen(sleeper, creationflags=subprocess.CREATE_NO_WINDOW)
    bystander = subprocess.Popen(sleeper, creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        identity = from_handle(owner.pid, owner._handle)
        write_session(library.root, "profile", owner, identity, children=(game,))
        with pytest.raises(ManagerError, match="No live session"):
            agent_launch.stop(library.root, bystander.pid, timeout=1)
        assert bystander.poll() is None and owner.poll() is None
        status = agent_launch.status(library.root)
        assert [session["helper_pid"] for session in status["sessions"]] == [owner.pid]
        game.terminate()  # stands in for kill-on-close of the helper-owned job
        result = agent_launch.stop(library.root, game.pid, timeout=10)
        assert result["stopped"] and result["helper_pid"] == owner.pid and result["liveness"] == "exited"
        assert owner.wait(timeout=10) is not None and bystander.poll() is None
        assert agent_launch.status(library.root)["sessions"] == []
    finally:
        for process in (owner, game, bystander):
            if process.poll() is None:
                process.kill()
            process.wait(timeout=10)


@pytest.mark.skipif(os.name != "nt", reason="Windows process identities")
def test_stop_refuses_a_recycled_or_mismatched_identity(tmp_path):
    library = Library(tmp_path / "library", create=True)
    owner = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"], creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        write_session(library.root, "profile", owner, {"pid": owner.pid, "created": "1"})
        with pytest.raises(ManagerError):
            agent_launch.stop(library.root, owner.pid, timeout=1)
        assert owner.poll() is None
    finally:
        owner.kill()
        owner.wait(timeout=10)


def test_instances_lists_mo2_instances_read_only(instance, tmp_path):
    root = tmp_path / "LocalAppData" / "ModOrganizer"
    (root / "Fixture").mkdir(parents=True)
    (root / "Fixture" / "ModOrganizer.ini").write_bytes((instance.source / "ModOrganizer.ini").read_bytes()
        .replace(b"base_directory=", b"base_directory=" + str(instance.source).encode() + b"\nignored="))
    (root / "cache").mkdir()
    before = tree_bytes(root)
    found = agent_launch.instances(root)
    assert [item["name"] for item in found] == ["Fixture"]
    assert found[0]["game_path"] == str(instance.game) and found[0]["mods"] == str(instance.source / "mods")
    assert found[0]["profiles"] == ["Alternate", "Default"] and found[0]["selected_profile"] == "Default"
    assert tree_bytes(root) == before


def test_cli_prints_machine_readable_json(instance, monkeypatch, capsys):
    monkeypatch.setattr(agent_launch, "sync", lambda source, library: {"library": str(library), "profiles": []})
    assert agent_launch.main(["sync", "--mo2", str(instance.source), "--library", str(instance.library)]) == 0
    assert json.loads(capsys.readouterr().out)["library"] == str(instance.library)
    def failed(*args, **kwargs):
        raise ManagerError("fixture failure")
    monkeypatch.setattr(agent_launch, "sync", failed)
    assert agent_launch.main(["sync", "--mo2", str(instance.source), "--library", str(instance.library)]) == 1
    assert json.loads(capsys.readouterr().out) == {"error": "fixture failure"}


def test_dry_run_plans_without_starting(instance, monkeypatch):
    synced(instance)
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.ensure", lambda cancel, progress: instance.tmp / "bundle")
    monkeypatch.setattr("creation_lib.mod_manager.runtime.start", lambda *a, **k: pytest.fail("dry run started"))
    result = agent_launch.launch(instance.library, "Default", "f4se", dry_run=True, user_targets=instance.targets)
    assert result["state"] == "planned" and "pid" not in result and result["mapped_files"] > 0
    assert not list((instance.library / "runtime" / "sessions").glob("*/request.json"))
    assert Path(result["mappings"]).is_file()


def test_import_saves_copies_selected_and_latest_pairs_into_owned_saves_read_only(instance):
    import time
    from .test_saves import save_fixture
    result, library = synced(instance)
    profile_id = next(p["id"] for p in result["profiles"] if p["name"] == "Default")
    source = instance.user / "Saves"
    source.mkdir()
    save_fixture(source / "Old.fos")
    (source / "Old.f4se").write_bytes(b"old companion")
    time.sleep(.02)
    save_fixture(source / "New.fos")
    os.utime(source / "New.fos", ns=((source / "Old.fos").stat().st_mtime_ns + 10**9,) * 2)
    before = tree_bytes(source)
    copied = agent_launch.import_saves(instance.library, "Default", ["Old.fos"], folder=source)
    owned = instance.library / "runtime" / "profiles" / profile_id / "saves"
    assert copied["copied"] == ["Old.fos"] and copied["saves"] == str(owned)
    assert (owned / "Old.f4se").read_bytes() == b"old companion" and tree_bytes(source) == before
    latest = agent_launch.import_saves(instance.library, "Default", [], latest=True, folder=source)
    assert latest["copied"] == ["New.fos"]
    again = agent_launch.import_saves(instance.library, "Default", ["Old.fos", "New.fos"], folder=source)
    assert again["copied"] == [] and again["skipped"] == [{"save": "Old.fos", "reason": "already in owned saves"},
                                                          {"save": "New.fos", "reason": "already in owned saves"}]
    with pytest.raises(ManagerError, match="Missing.fos"):
        agent_launch.import_saves(instance.library, "Default", ["Missing.fos"], folder=source)
    with pytest.raises(ManagerError, match="Choose"):
        agent_launch.import_saves(instance.library, "Default", [], folder=source)
    assert tree_bytes(source) == before


def test_cli_import_saves_forwards_choices(instance, monkeypatch, capsys):
    calls = []
    monkeypatch.setattr(agent_launch, "import_saves", lambda *a, **k: calls.append((a, k)) or {"copied": []})
    assert agent_launch.main(["import-saves", "--library", str(instance.library), "--profile", "Default",
                              "--save", "A.fos", "--latest", "--folder", str(instance.tmp)]) == 0
    assert calls == [((instance.library, "Default", ["A.fos"]), {"latest": True, "folder": str(instance.tmp)})]
    assert json.loads(capsys.readouterr().out) == {"copied": []}


def test_helper_writes_crlf_plugin_and_ini_snapshots_without_doubling_carriage_returns(instance, monkeypatch):
    # MO2 profile files are CRLF; "\r\r\n" made FO4 read "*X.esm\r" and deactivate every mod plugin.
    synced(instance)
    library = Library(instance.library)
    _, plan, _ = captured_launch(instance, monkeypatch, accept_minimal_ini=True)
    assert any("\r\n" in text for _, text, kind in plan.snapshots if kind == "plugins")
    helper_run(library, plan, monkeypatch)
    owned = instance.library / "runtime" / "sessions" / plan.session_id / "output"
    for folder in ("plugins", "ini"):
        for path in (owned / folder).iterdir():
            assert b"\r\r" not in path.read_bytes(), path
    assert (owned / "plugins" / "plugins.txt").read_bytes().splitlines()[0] == b"*Fixture.esp"


def test_game_root_writes_route_to_owned_output_and_are_reported(instance, monkeypatch):
    # Engine screenshots (ScreenShotN.png) are written beside Fallout4.exe; a protected root swallowed them.
    synced(instance)
    library = Library(instance.library)
    result, plan, _ = captured_launch(instance, monkeypatch, accept_minimal_ini=True)
    owned = instance.library / "runtime" / "sessions" / plan.session_id / "output"
    assert result["output"]["game_root"] == str(owned / "game-root")
    mappings = json.loads(Path(result["mappings"]).read_text("utf-8"))
    assert mappings["mounts"][str(instance.game)] == str(owned / "game-root")
    adapter = helper_run(library, plan, monkeypatch)
    assert ("directory", str(owned / "game-root"), str(instance.game), True) in adapter.operations
    assert (owned / "game-root").is_dir() and not list((owned / "game-root").iterdir())
