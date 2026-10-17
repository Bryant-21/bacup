"""Automatic destination creation uses isolated app roots and synthetic sources."""
from pathlib import Path
import sqlite3

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.locations import default_library_path
from creation_lib.mod_manager.paths import ManagerError
from ui.mod_manager.workspace import ModManagerWorkspace
from ui.toolkit.settings import ToolkitSettings
from .fixtures import make_mo2
from .test_controller import finish


@pytest.fixture
def isolated(tmp_path, monkeypatch):
    app_data = tmp_path / "app data"
    monkeypatch.setattr("app.paths.get_db_dir", lambda: app_data)
    manager = ManagerController()
    manager.inspect_mo2(make_mo2(tmp_path))
    finish(manager)
    yield manager, app_data / "mod_manager" / "library"
    manager.cleanup()


def selected(manager):
    return {profile.id for profile in manager.import_preview.profiles}


def failure(manager):
    with pytest.raises(ManagerError):
        manager._future.result(timeout=10)
    manager.poll()
    assert manager.job_state == "failed"


def test_first_run_automatic_import_repeat_and_unchanged_source(isolated):
    manager, automatic = isolated
    source = Path(manager.import_preview.root)
    before = {path.relative_to(source): path.read_bytes() for path in source.rglob("*") if path.is_file()}
    assert default_library_path() == automatic and manager.resolve_destination("   ") == automatic
    assert not automatic.exists()
    manager.import_profiles(selected(manager))
    finish(manager)
    assert manager.library.root == automatic and len(manager.profiles) == 2
    assert all((automatic / name).is_dir() for name in ("mods", ".staging", "exports", "downloads"))
    assert not tuple((automatic / "mods").iterdir())
    manager.customize(manager.rows[0].id, notes="keep local organization")
    finish(manager)
    manager.import_profiles(selected(manager))
    finish(manager)
    assert "Imported 0 new profiles" in manager.status
    assert manager.rows[0].notes == "keep local organization"
    assert {path.relative_to(source): path.read_bytes() for path in source.rglob("*") if path.is_file()} == before


def test_custom_missing_parents_existing_library_and_blank_keeps_current(isolated, tmp_path):
    manager, automatic = isolated
    custom = tmp_path / "custom" / "missing parents" / "library"
    manager.import_profiles(selected(manager), str(custom))
    finish(manager)
    assert manager.library.root == custom and not automatic.exists()
    original_id = manager.library.library_id
    manager.import_profiles(selected(manager), " ")
    finish(manager)
    assert manager.library.root == custom and manager.library.library_id == original_id
    manager.library = None
    manager.import_profiles(selected(manager), custom)
    finish(manager)
    assert manager.library.library_id == original_id and "Imported 0 new profiles" in manager.status


@pytest.mark.parametrize("kind", ["occupied", "file", "malformed_marker"])
def test_unexpected_destination_preserved_and_no_default_fallback(isolated, tmp_path, kind):
    manager, automatic = isolated
    custom = tmp_path / "custom"
    if kind == "file":
        custom.write_bytes(b"unexpected file")
        protected = custom
    else:
        custom.mkdir()
        protected = custom / (Library.MARKER if kind == "malformed_marker" else "keep.txt")
        protected.write_bytes(b"unexpected file")
    manager.import_profiles(selected(manager), custom)
    failure(manager)
    assert manager.library is None and str(custom) in manager.error
    assert protected.read_bytes() == b"unexpected file" and not automatic.exists()
    if custom.is_dir():
        assert set(custom.iterdir()) == {protected}
    else:
        assert custom.is_file()


@pytest.mark.parametrize("selected_ids", [set(), {"stale preview id"}])
def test_invalid_selection_creates_no_destination(isolated, selected_ids):
    manager, automatic = isolated
    with pytest.raises(ManagerError, match="current import preview"):
        manager.import_profiles(selected_ids)
    assert not automatic.exists() and not manager.busy


def test_source_destination_overlap_creates_nothing(isolated):
    manager, automatic = isolated
    target = Path(manager.import_preview.root) / "unsafe destination"
    with pytest.raises(ManagerError, match="outside the MO2"):
        manager.import_profiles(selected(manager), target)
    assert not target.exists() and not automatic.exists()


@pytest.mark.parametrize("error", [PermissionError("fixture access denied"), OSError(28, "fixture disk full")])
def test_creation_errors_report_path_and_allow_retry(isolated, monkeypatch, error):
    manager, automatic = isolated
    from creation_lib.mod_manager.backend import mod_manager_native
    library_open = mod_manager_native.library_open
    def blocked(path, *args, **kwargs):
        if Path(path) == automatic:
            raise error
        return library_open(path, *args, **kwargs)
    with monkeypatch.context() as patch:
        patch.setattr(mod_manager_native, "library_open", blocked)
        manager.import_profiles(selected(manager))
        failure(manager)
    assert str(automatic) in manager.error and "permissions and available disk space" in manager.error
    assert not automatic.exists() and manager.library is None
    manager.import_profiles(selected(manager))
    finish(manager)
    assert manager.library.root == automatic and len(manager.profiles) == 2


def test_database_initialization_disk_error_keeps_owned_marker_retryable(isolated, monkeypatch):
    manager, automatic = isolated
    with monkeypatch.context() as patch:
        patch.setattr("creation_lib.mod_manager.backend.mod_manager_native.Database", lambda *a, **kw: (_ for _ in ()).throw(sqlite3.OperationalError("database or disk is full")))
        manager.import_profiles(selected(manager))
        failure(manager)
    assert "disk is full" in manager.error and str(automatic) in manager.error
    assert (automatic / Library.MARKER).is_file() and manager.library is None
    manager.import_profiles(selected(manager))
    finish(manager)
    assert len(manager.profiles) == 2


@pytest.mark.parametrize("outcome", ["cancelled", "disk_error"])
def test_import_failure_rolls_back_profiles_and_reuses_created_library(isolated, monkeypatch, outcome):
    manager, automatic = isolated
    original = Library.import_mo2
    def interrupted(library, preview, chosen, cancel=None, progress=None):
        def report(*args):
            if outcome == "cancelled":
                manager.cancel()
            else:
                raise sqlite3.OperationalError("database or disk is full")
        return original(library, preview, chosen, cancel, report)
    with monkeypatch.context() as patch:
        patch.setattr(Library, "import_mo2", interrupted)
        manager.import_profiles(selected(manager))
        with pytest.raises(ManagerError):
            manager._future.result(timeout=10)
        manager.poll()
    assert manager.job_state == ("cancelled" if outcome == "cancelled" else "failed")
    assert manager.library is None and Library(automatic).profiles() == []
    if outcome == "disk_error":
        assert "disk is full" in manager.error and str(automatic) in manager.error
    manager.import_profiles(selected(manager))
    finish(manager)
    assert len(manager.profiles) == 2 and not tuple((automatic / "mods").iterdir())


@pytest.mark.parametrize("mode", ["default", "custom", "missing_saved"])
def test_workspace_restart_settings_roundtrip_and_missing_saved_creation(isolated, tmp_path, mode):
    _, automatic = isolated
    target = automatic if mode == "default" else tmp_path / "saved custom" / "library"
    workspace = ModManagerWorkspace()
    settings_path = tmp_path / "isolated settings.json"
    settings = ToolkitSettings(path=settings_path, editor_settings_path=tmp_path / "absent editor.json")
    try:
        workspace.manager.inspect_mo2(tmp_path / "fixture-mo2")
        finish(workspace.manager)
        if mode == "missing_saved":
            workspace.apply_settings({"library_path": str(target)})
            workspace.draw()
            failure(workspace.manager)
            assert workspace._library_path == str(target) and not target.exists()
        else:
            workspace.manager.import_profiles(selected(workspace.manager), "" if mode == "default" else target)
            workspace._show_imported_profiles = True
            finish(workspace.manager)
            workspace.draw()
        settings.set_workspace_settings("mod_manager", workspace.collect_settings())
        settings.save()
    finally:
        workspace.cleanup()
    loaded = ToolkitSettings(path=settings_path, editor_settings_path=tmp_path / "absent editor.json")
    restored = ModManagerWorkspace()
    try:
        restored.apply_settings(loaded.get_workspace_settings("mod_manager"))
        assert restored._library_path == str(target)
        restored.draw()
        if mode == "missing_saved":
            failure(restored.manager)
            assert not target.exists()
            restored.manager.inspect_mo2(tmp_path / "fixture-mo2")
            finish(restored.manager)
            restored.manager.import_profiles(selected(restored.manager), restored._library_path)
            finish(restored.manager)
        else:
            finish(restored.manager)
        assert restored.manager.library.root == target
        assert len(restored.manager.profiles) == 2
        if mode != "default":
            assert not automatic.exists()
        else:
            assert automatic.is_dir()
    finally:
        restored.cleanup()


def test_invalid_saved_destination_stays_selected_until_explicit_correction(isolated, tmp_path):
    _, automatic = isolated
    occupied = tmp_path / "saved invalid"
    occupied.mkdir()
    sentinel = occupied / "keep.txt"
    sentinel.write_bytes(b"untouched")
    workspace = ModManagerWorkspace()
    try:
        workspace.apply_settings({"library_path": str(occupied)})
        workspace.draw()
        failure(workspace.manager)
        assert str(occupied) in workspace.manager.error
        assert workspace.collect_settings()["library_path"] == str(occupied)
        workspace.manager.inspect_mo2(tmp_path / "fixture-mo2")
        finish(workspace.manager)
        workspace.manager.import_profiles(selected(workspace.manager), workspace._library_path)
        failure(workspace.manager)
        assert sentinel.read_bytes() == b"untouched" and not automatic.exists()
        workspace._library_path = ""
        workspace.manager.import_profiles(selected(workspace.manager), workspace._library_path)
        workspace._show_imported_profiles = True
        finish(workspace.manager)
        workspace.draw()
        assert workspace.collect_settings()["library_path"] == str(automatic)
        assert sentinel.read_bytes() == b"untouched"
    finally:
        workspace.cleanup()
