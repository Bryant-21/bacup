"""Standalone lifecycle uses only generated game/layout/package fixtures."""
import json
import threading

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.instances import create
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from creation_lib.mod_manager.runtime import prepare
from .test_runtime import pe


def game_fixture(tmp_path):
    game = tmp_path / "game"
    (game / "Data").mkdir(parents=True)
    pe(game / "Fallout4.exe")
    pe(game / "f4se_loader.exe")
    return game


def test_standalone_create_install_restart_repeat_and_local_profiles(tmp_path):
    game = game_fixture(tmp_path)
    source = {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()}
    library = Library(tmp_path / "library", create=True)
    profile_id = create(library, "fo4", game)
    profile = library.profiles()[0]
    assert profile.id == profile_id and profile.name == "Default" and not profile.source
    assert profile.files["_modbox_game"] == "Fallout 4" and profile.files["plugins.txt"] == ""
    assert len(library.executables(profile.files["_modbox_instance_id"])) == 2
    package_root = tmp_path / "fixture package"
    package_root.mkdir()
    (package_root / "test.txt").write_bytes(b"fixture")
    package = Package(package_root)
    try:
        library.install(package, package.plain_plan(), "Standalone Mod", profile_id)
    finally:
        package.close()
    clone = library.create_profile("Clone", profile_id)
    blank = library.create_profile("Blank", template=profile_id)
    assert library.rows(clone)[0].enabled and not library.rows(blank)[0].enabled
    assert next(p for p in library.profiles() if p.id == blank).files["_modbox_instance_id"] == profile.files["_modbox_instance_id"]
    reopened = Library(library.root)
    assert create(reopened, "fo4", game) in {p.id for p in reopened.profiles()}
    assert len(reopened.profiles()) == 3 and len(reopened.configurations()) == 1
    assert {str(path): path.read_bytes() for path in game.rglob("*") if path.is_file()} == source


def test_standalone_cancel_rolls_back_instance_profile_and_executables(tmp_path):
    library = Library(tmp_path / "library", create=True)
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(Cancelled):
        create(library, "fo4", game_fixture(tmp_path), cancel=cancel)
    assert library.profiles() == library.configurations() == library.executables() == []


def test_standalone_game_library_overlap_is_rejected_before_creation(tmp_path):
    game = game_fixture(tmp_path)
    manager = ManagerController()
    try:
        with pytest.raises(ManagerError, match="separate"):
            manager.create_instance("fo4", game, game / "manager")
        assert not (game / "manager").exists()
    finally:
        manager.cleanup()


def test_standalone_profile_prepares_native_mapping_without_mo2(tmp_path):
    game = game_fixture(tmp_path)
    library = Library(tmp_path / "library", create=True)
    profile_id = create(library, "fo4", game)
    instance = library.profiles()[0].files["_modbox_instance_id"]
    executable = next(e for e in library.executables(instance) if e.binary.endswith("Fallout4.exe"))
    bundle = tmp_path / "fixture USVFS"
    bundle.mkdir()
    for name, machine in (("usvfs_x64.dll", 0x8664), ("usvfs_x86.dll", 0x14c), ("usvfs_proxy_x64.exe", 0x8664), ("usvfs_proxy_x86.exe", 0x14c)):
        pe(bundle / name, machine)
    plan = prepare(library, profile_id, executable, str(game), str(bundle), user_targets=(tmp_path / "global plugins", tmp_path / "global ini"))
    assert {"Fallout4.ini", "Fallout4Prefs.ini"} == set(plan.minimal_ini)
    assert any(name == "plugins.txt" for name, _, _ in plan.snapshots)
    assert json.loads(library.profiles()[0].files["_modbox_minimal_inis"])


def test_standalone_fo4_uses_read_only_game_default_and_local_loose_file_settings(tmp_path):
    game = game_fixture(tmp_path)
    default = game / "Fallout4_default.ini"
    text = "[General]\nGeneratedDefault=1\n[Archive]\nsResourceArchiveList=Fixture.ba2\n"
    default.write_text(text, encoding="utf-8")
    original = default.read_bytes()
    library = Library(tmp_path / "library", create=True)
    profile_id = create(library, "fo4", game)
    files = library.profiles()[0].files
    assert files["Fallout4.ini"] == original.decode("utf-8")
    assert "bInvalidateOlderFiles=1" in files["Fallout4Custom.ini"]
    assert json.loads(files["_modbox_minimal_inis"]) == ["Fallout4Prefs.ini"]
    assert default.read_bytes() == original
    # Repeat setup does not re-read changed defaults or overwrite local edits.
    default.write_text("not a valid INI", encoding="utf-8")
    assert create(library, "fo4", game) == profile_id
    assert library.profiles()[0].files["Fallout4.ini"] == original.decode("utf-8")
