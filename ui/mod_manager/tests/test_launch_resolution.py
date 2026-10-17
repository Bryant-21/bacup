"""Disposable installation selection; no game or imported executable is run."""
from dataclasses import replace
from pathlib import Path
import threading

import pytest

from creation_lib.mod_manager import f4se_setup, launch_targets, runtime
from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.launch_resolution import resolve
from creation_lib.mod_manager.launcher import Executable
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_controller import finish
from .test_f4se_setup import package
from .test_pe_metadata import fixture_version, image


def installation(root, build=(1, 10, 163, 0)):
    (root / "Data/Scripts").mkdir(parents=True, exist_ok=True)
    image(root / "Fallout4.exe", version=build)
    release = f4se_setup.CATALOG[build[:3]]
    image(root / "f4se_loader.exe", version=tuple(map(int, release["version"].split("."))) + (0,))
    image(root / f4se_setup.runtime_name(build))
    if release["steam_loader"]:
        image(root / "f4se_steam_loader.dll")
    (root / "Data/Scripts/F4SE.pex").write_bytes(b"generated inert script")
    (root / "Fallout4_default.ini").write_text("[General]\nInstallation=" + root.name + "\n")
    return root


@pytest.fixture
def configured(tmp_path, monkeypatch):
    monkeypatch.setattr("creation_lib.mod_manager.pe_metadata.file_version", fixture_version)
    games = [installation(tmp_path / "old"), installation(tmp_path / "selected", (1, 11, 240, 0))]
    library = Library(tmp_path / "library", create=True)
    profile = create(library, "fo4", games[0])
    return library, profile, games


def test_selected_loader_authoritative_switch_repeat_isolated_state_and_real_plan(configured, tmp_path, monkeypatch):
    library, profile, games = configured
    before = {str(p): p.read_bytes() for game in games for p in game.rglob("*") if p.is_file()}
    bundle = tmp_path / "inert native bundle"
    bundle.mkdir()
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.ensure", lambda *args: bundle)
    users = (tmp_path / "user-plugins", tmp_path / "user-inis")
    for user in users:
        user.mkdir()
    monkeypatch.setattr(runtime, "fo4_user_targets", lambda: users)
    manager = ManagerController()
    try:
        manager.library, manager.profile_id = library, profile
        executable = library.save_executable(Executable("external", "Preconfigured F4SE", str(games[1] / "f4se_loader.exe"), "-v", str(games[1])))
        manager.refresh()
        original_rows = library.rows(profile)
        manager.prepare_launch(executable.id, str(games[0]))
        finish(manager)
        target_id, plan = manager.target_id, manager.launch_review
        assert Path(plan.game_root) == games[1] and plan.target_id == target_id
        assert Path(plan.library_root) == library.root / ".launch-targets" / target_id
        assert plan.launch.argv == (executable.binary, "-v")
        assert manager.instance_configuration["game"]["gamepath"] == str(games[1])
        assert "Installation=selected" in manager.profiles[0].files["Fallout4.ini"]
        assert library.rows(profile) == original_rows
        view = manager.context_library
        current = view.profile_snapshot(profile)[0].files["Fallout4.ini"]
        view.save_ini(profile, "Fallout4.ini", "[General]\nKept=1\n", current)
        manager.launch_review = None  # cancel review: no process/session/output
        assert not list(library.root.glob(".launch-targets/*/runtime"))
        manager.select_target("")
        manager.prepare_launch(executable.id, str(games[0]))
        finish(manager)
        assert manager.target_id == target_id and len(manager.targets) == 1
        assert "Kept=1" in manager.profiles[0].files["Fallout4.ini"]
        assert library.executables() == [executable]
        assert {str(p): p.read_bytes() for game in games for p in game.rglob("*") if p.is_file()} == before
    finally:
        manager.cleanup()


def test_staged_loader_uses_explicit_association_not_parent(configured, tmp_path):
    library, _, games = configured
    identifier = launch_targets.apply(library, launch_targets.review(library, "Managed", games[0]))
    target = f4se_setup.prepare(library, identifier, archive=package(tmp_path))
    imported = replace(target.executable, id="copied-definition", title="Staged F4SE", working_directory="")
    result = resolve(library, imported)
    assert result.target_id == identifier and result.game_root == str(games[0])
    assert result.executable.id == target.executable.id
    assert result.executable.working_directory == str(games[0])
    orphan = library.root / "unassociated"
    orphan.mkdir()
    image(orphan / "f4se_loader.exe")
    with pytest.raises(ManagerError, match="registered target association"):
        resolve(library, replace(imported, binary=str(orphan / "f4se_loader.exe")))


@pytest.mark.parametrize("fault", ["cancel", "commit", "missing", "x86"])
def test_invalid_cancelled_failed_resolution_never_selects_old_data(configured, monkeypatch, fault):
    library, _, games = configured
    exe = Executable("fixture", "F4SE", str(games[1] / "f4se_loader.exe"))
    cancel = threading.Event()
    if fault == "cancel":
        cancel.set()
    elif fault == "commit":
        monkeypatch.setattr(launch_targets, "_checkpoint", lambda: (_ for _ in ()).throw(OSError("generated metadata failure")))
    elif fault == "missing":
        (games[1] / "Fallout4.exe").unlink()
    elif fault == "x86":
        from .test_runtime import pe
        pe(games[1] / "Fallout4.exe", 0x14c)
    with pytest.raises((Cancelled, ManagerError, OSError)):
        resolve(library, exe, cancel=cancel)
    assert launch_targets.records(library) == []


@pytest.mark.parametrize("version", [None, (9, 9, 9, 0)])
@pytest.mark.parametrize("build", [(1, 11, 240, 0), (1, 2, 3, 0)])
def test_existing_loader_launch_does_not_require_catalog_version(configured, tmp_path, monkeypatch, version, build):
    library, profile, games = configured
    image(games[1] / "f4se_loader.exe", version=version)
    image(games[1] / "Fallout4.exe", version=build)
    before = {str(p): p.read_bytes() for p in games[1].rglob("*") if p.is_file()}
    executable = Executable("external", "Custom F4SE", str(games[1] / "f4se_loader.exe"))
    resolution = resolve(library, executable)
    context = launch_targets.TargetLibrary(library, resolution.target_id)
    monkeypatch.setattr("creation_lib.mod_manager.native_builder.ensure", lambda *args: tmp_path / "bundle")
    users = (tmp_path / "plugins", tmp_path / "ini")
    for user in users:
        user.mkdir()
    plan = runtime.prepare(context, profile, resolution.executable, resolution.game_root, "", user_targets=users)
    assert plan.game_root == str(games[1])
    assert plan.launch.argv == (executable.binary,)
    assert not plan.root_files and not plan.experimental
    assert f4se_setup.validate_launch(context.target())["managed"] is False
    assert {str(p): p.read_bytes() for p in games[1].rglob("*") if p.is_file()} == before


def test_unknown_existing_loader_reaches_helper_start(configured, tmp_path):
    from .test_runtime import pe, request, FakeVFS
    from creation_lib.mod_manager.runtime_helper import run
    library, profile, games = configured
    image(games[1] / "f4se_loader.exe", version=None)
    resolution = resolve(library, Executable("external", "Unknown F4SE", str(games[1] / "f4se_loader.exe")))
    context = launch_targets.TargetLibrary(library, resolution.target_id)
    bundle = tmp_path / "inert bundle"
    bundle.mkdir()
    for name, machine in (("usvfs_x64.dll", 0x8664), ("usvfs_x86.dll", 0x14c), ("usvfs_proxy_x64.exe", 0x8664), ("usvfs_proxy_x86.exe", 0x14c)):
        pe(bundle / name, machine)
    plan = runtime.prepare(context, profile, resolution.executable, resolution.game_root, str(bundle),
                           user_targets=(tmp_path / "plugins", tmp_path / "ini"))
    path = request(context, plan)
    assert run(path, native_factory=FakeVFS, wait=lambda _: None) == 0


def test_active_session_blocks_association(configured, monkeypatch):
    library, _, games = configured
    monkeypatch.setattr(runtime, "active_sessions", lambda root: [(None, {"state": "running"})])
    with pytest.raises(ManagerError, match="runtime"):
        resolve(library, Executable("fixture", "F4SE", str(games[1] / "f4se_loader.exe")))


def test_active_session_allows_existing_launch_target(configured, monkeypatch):
    library, _, games = configured
    executable = Executable("fixture", "F4SE", str(games[1] / "f4se_loader.exe"))
    first = resolve(library, executable)
    monkeypatch.setattr(runtime, "active_sessions", lambda root: [(None, {"state": "running"})])
    second = resolve(library, executable, selected_target=first.target_id)
    assert second == first
    assert len(launch_targets.records(library)) == 1
