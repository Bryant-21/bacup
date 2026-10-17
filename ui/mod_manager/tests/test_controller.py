from pathlib import Path

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.paths import ManagerError
from ui.mod_manager.workspace import ModManagerWorkspace
from ui.toolkit.workspaces import create_workspaces
from .fixtures import make_mo2, make_package


def finish(manager):
    manager._future.result(timeout=10)
    manager.poll()
    assert not manager.error, manager.error


def test_supported_controller_flow_and_repeated_import(tmp_path):
    manager = ManagerController()
    try:
        manager.open_library(tmp_path / "library", create=True)
        finish(manager)
        manager.inspect_mo2(make_mo2(tmp_path))
        finish(manager)
        selected = {manager.import_preview.profiles[0].id}
        manager.import_profiles(selected)
        finish(manager)
        assert len(manager.profiles) == 1
        manager.import_profiles(selected)
        finish(manager)
        assert "0 new profiles" in manager.status
        first = manager.filtered("base")[0]
        assert manager.filtered("base") is manager.filtered("base")
        manager.customize(first.id, tags="favorite", group="Custom")
        finish(manager)
        assert manager.filtered("favorite", "Custom")[0].id == first.id
        manager.create_profile("Experiment", clone=True)
        finish(manager)
        manager.prepare_install(make_package(tmp_path))
        finish(manager)
        assert manager.prepared.fomod is not None
        assert manager.prepared.environment.files["fixture.esp"] == "Active"
        manager.prepared.environment.files["absent.esp"] = "Missing"
        with pytest.raises(ManagerError, match="Review"):
            manager.install("Fixture")
        manager.review_install()
        finish(manager)
        manager.install("Fixture")
        finish(manager)
        assert manager.prepared is None
        assert any(row.name == "Fixture" for row in manager.rows)
        manager.preview_export()
        finish(manager)
        manager.export()
        finish(manager)
        assert "staged successfully" in manager.status
    finally:
        manager.cleanup()


def test_failed_job_visible_and_installer_close_safe(tmp_path):
    manager = ManagerController()
    try:
        manager.inspect_mo2(tmp_path / "missing")
        try:
            manager._future.result(timeout=10)
        except ManagerError:
            pass
        manager.poll()
        assert manager.job_state == "failed" and "missing" in manager.error
        manager.prepare_install(make_package(tmp_path))
        finish(manager)
        package = manager.prepared.package
        manager.discard_install()
        assert manager.prepared is None and package._zip is None
    finally:
        manager.cleanup()


def test_workspace_registration_settings_and_panel_contract():
    workspaces = create_workspaces(workspace_ids=("mod_manager",))
    assert len(workspaces) == 1 and isinstance(workspaces[0], ModManagerWorkspace)
    workspace = workspaces[0]
    try:
        workspace.apply_settings({"library_path": "fixture-path", "row_height": 35})
        assert workspace.collect_settings()["library_path"] == "fixture-path"
        assert workspace.collect_settings()["row_height"] == 35
        assert all(window.label.endswith("##mod_manager") and not window.call_begin_end for window in workspace.get_dockable_windows())
        assert workspace.get_user_guide() is not None
    finally:
        workspace.cleanup()


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_partial_migration_refreshes_committed_ownership_and_invalidates_file_index(tmp_path, monkeypatch, outcome):
    import shutil
    from types import SimpleNamespace
    manager = ManagerController()
    try:
        manager.open_library(tmp_path / "library", create=True)
        finish(manager)
        manager.inspect_mo2(make_mo2(tmp_path))
        finish(manager)
        manager.import_profiles({manager.import_preview.profiles[0].id})
        finish(manager)
        manager.inspect_files()
        finish(manager)
        assert manager.file_index
        selected = [row.id for row in manager.rows if row.kind == "external"]
        manager.review_migration(selected)
        finish(manager)
        low_space, usage = False, shutil.disk_usage
        def checkpoint(name):
            nonlocal low_space
            if name == "committed":
                if outcome == "cancelled":
                    manager.cancel()
                else:
                    low_space = True
        monkeypatch.setattr(manager.library.owned, "_checkpoint", checkpoint)
        monkeypatch.setattr("creation_lib.mod_manager.migration.shutil.disk_usage", lambda path: SimpleNamespace(free=0) if low_space else usage(path))
        manager.migrate()
        manager._future.exception(timeout=10)
        manager.poll()
        assert manager.job_state == outcome
        assert sum(row.kind == "owned" for row in manager.rows) == 1 and len(manager.origins) == 1
        assert manager.file_index is None and manager.migration_review is None
        assert "1 / 3 sources" in (manager.status if outcome == "cancelled" else manager.error)
    finally:
        manager.cleanup()


def test_classic_mo2_plugin_list_treats_listed_plugins_as_active(tmp_path):
    source = make_mo2(tmp_path)
    ini = source / "ModOrganizer.ini"
    ini.write_text(ini.read_text().replace("Fallout 4", "Fallout New Vegas"))
    for profile in (source / "profiles").iterdir():
        (profile / "plugins.txt").write_text("Fixture.esp\n")
    manager = ManagerController()
    try:
        manager.open_library(tmp_path / "library", create=True)
        finish(manager)
        manager.inspect_mo2(source)
        finish(manager)
        manager.import_profiles({manager.import_preview.profiles[0].id})
        finish(manager)
        manager.prepare_install(make_package(tmp_path))
        finish(manager)
        assert manager.prepared.environment.files["fixture.esp"] == "Active"
    finally:
        manager.cleanup()


def test_cancelled_controller_install_reports_rollback(tmp_path):
    from creation_lib.mod_manager.paths import Cancelled
    manager = ManagerController()
    try:
        manager.open_library(tmp_path / "library", create=True)
        finish(manager)
        manager.create_profile("Main")
        finish(manager)
        manager.prepare_install(make_package(tmp_path))
        finish(manager)
        manager.prepared.environment.files["absent.esp"] = "Missing"
        manager.prepared.environment.files["fixture.esp"] = "Missing"
        manager.review_install()
        finish(manager)
        original = manager.library._copy_plan
        def cancel_copy(sources, destination, cancel=None, progress=None):
            return original(sources, destination, cancel, lambda fraction, target: cancel.set())
        manager.library._copy_plan = cancel_copy
        manager.install("Cancelled")
        with pytest.raises(Cancelled):
            manager._future.result(timeout=10)
        manager.poll()
        assert manager.job_state == "cancelled" and not manager.rows
        assert manager.prepared is not None
        assert not any((manager.library.root / ".staging").iterdir())
    finally:
        manager.cleanup()


@pytest.mark.parametrize("kind", ["malformed", "executable", "duplicate-config"])
def test_manual_layout_explicitly_bypasses_rules_excludes_scripts_and_keeps_review_guards(tmp_path, kind):
    package = tmp_path / "manual package"
    (package / "wrapper" / "fomod").mkdir(parents=True)
    (package / "wrapper" / "Data").mkdir()
    config = package / "wrapper" / "fomod" / "ModuleConfig.xml"
    config.write_text("<not-valid" if kind == "malformed" else "<config><moduleName>Fixture</moduleName></config>")
    if kind == "executable":
        (config.parent / "script.py").write_text("raise RuntimeError('Must never run')")
    if kind == "duplicate-config":
        (package / "fomod").mkdir()
        (package / "fomod" / "ModuleConfig.xml").write_text(config.read_text())
    payload = package / "wrapper" / "Data" / "fixture.txt"
    payload.write_bytes(b"generated manual payload")
    original = {str(path): path.read_bytes() for path in package.rglob("*") if path.is_file()}
    manager = ManagerController()
    try:
        manager.open_library(tmp_path / "library", create=True)
        finish(manager)
        manager.create_profile("Manual")
        finish(manager)
        manager.prepare_install(package)
        assert isinstance(manager._future.exception(timeout=10), Exception)
        manager.poll()
        assert manager.job_state == "failed" and manager.prepared is None
        manager.prepare_install(package, manual=True)
        finish(manager)
        assert manager.prepared.manual and manager.prepared.fomod is None
        assert all("fomod" not in entry.path.casefold() for entry in manager.prepared.source_tree.entries)
        manager.review_install("wrapper/Data")
        finish(manager)
        assert manager.install_plan == {"fixture.txt": "wrapper/Data/fixture.txt"}
        manager.install("Manual fixture")
        finish(manager)
        assert (Path(manager.rows[0].source) / "fixture.txt").read_bytes() == payload.read_bytes()
        assert {str(path): path.read_bytes() for path in package.rglob("*") if path.is_file()} == original
    finally:
        manager.cleanup()


def test_reviewed_reinstall_uninstall_and_restore_controller_flow(tmp_path):
    manager = ManagerController()
    try:
        manager.open_library(tmp_path / "library", create=True)
        finish(manager)
        manager.create_profile("Main")
        finish(manager)
        package = make_package(tmp_path)
        manager.prepare_install(package)
        finish(manager)
        manager.prepared.environment.files["absent.esp"] = "Missing"
        manager.prepared.environment.files["fixture.esp"] = "Missing"
        manager.review_install()
        finish(manager)
        manager.install("Owned Fixture")
        finish(manager)
        mod = manager.rows[0]
        manager.create_profile("Clone", clone=True)
        finish(manager)
        manager.prepare_install(package, replace_id=mod.id)
        finish(manager)
        assert manager.prepared.replacement_review.mod_id == mod.id
        manager.prepared.environment.files["absent.esp"] = "Missing"
        manager.prepared.environment.files["fixture.esp"] = "Missing"
        manager.review_install()
        finish(manager)
        manager.install("Owned Fixture")
        finish(manager)
        assert manager.rows[0].id == mod.id and len(manager.recovery_history) == 1
        manager.review_owned(mod.id)
        finish(manager)
        assert len(manager.lifecycle_review.profiles) == 2
        manager.uninstall()
        finish(manager)
        assert not manager.rows and len(manager.recovery_history) == 2
        entry = next(entry for entry in manager.recovery_history if entry["kind"] == "uninstall")
        manager.review_restore(entry["id"])
        finish(manager)
        manager.restore()
        finish(manager)
        assert manager.rows[0].id == mod.id
    finally:
        manager.cleanup()


def test_post_commit_failure_reports_committed_state_and_refreshes_library(tmp_path, monkeypatch):
    from creation_lib.mod_manager.transactions import CommittedTransaction
    manager = ManagerController()
    try:
        manager.open_library(tmp_path / "library", create=True)
        finish(manager)
        manager.create_profile("Main")
        finish(manager)
        manager.prepare_install(make_package(tmp_path))
        finish(manager)
        manager.prepared.environment.files.update({"fixture.esp": "Missing", "absent.esp": "Missing"})
        manager.review_install()
        finish(manager)
        def interrupted(name):
            if name == "committed":
                raise OSError("fixture finalization failure")
        monkeypatch.setattr(manager.library.owned, "_checkpoint", interrupted)
        manager.install("Committed Fixture")
        with pytest.raises(CommittedTransaction):
            manager._future.result(timeout=10)
        manager.poll()
        assert manager.job_state == "committed" and manager.prepared is None
        assert manager.rows[0].name == "Committed Fixture" and "committed" in manager.error
        manager.open_library(manager.library.root)
        finish(manager)
        assert manager.library.recovery_report and manager.rows[0].name == "Committed Fixture"
    finally:
        manager.cleanup()
