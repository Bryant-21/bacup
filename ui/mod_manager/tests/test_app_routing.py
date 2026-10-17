"""Real registry/entrypoint routing with isolated settings and disposable package paths."""
from types import SimpleNamespace

import pytest

from creation_lib.mod_manager.package import validate_intake
from ui.mod_manager import archive_intake
from creation_lib.mod_manager.paths import ManagerError
from ui.mod_manager.workspace import ModManagerWorkspace
from ui.toolkit.app import ToolkitApp
from ui.toolkit.settings import ToolkitSettings
from ui.toolkit.variants import get_variant
from ui.toolkit.workspaces import create_workspaces
from .fixtures import make_package


@pytest.mark.parametrize("extension", ["zip", "7z", "rar", "fomod"])
def test_startup_mod_archive_routes_to_real_manager_registry(tmp_path, extension):
    path = tmp_path / ("generated." + extension)
    path.write_bytes(b"inert routing fixture")
    target = ToolkitApp._resolve_launch_target(str(path))
    assert target == ("mod_manager", str(path.resolve()))
    workspace = create_workspaces(workspace_ids=(target[0],))[0]
    try:
        assert isinstance(workspace, ModManagerWorkspace) and workspace.name == "Modlist Manager"
        workspace.open_file(target[1])
        assert workspace._incoming_package == str(path)
        assert workspace.manager.library is None and not workspace.manager.busy
    finally:
        workspace.cleanup()


@pytest.mark.parametrize("launch_path", [None, "nxm://fallout4/mods/1/files/2"])
def test_standalone_entrypoint_skips_authoring_wizard_and_creates_only_new_manager(tmp_path, monkeypatch, launch_path):
    from ui.toolkit import __main__ as entry
    settings = ToolkitSettings(shared_path=tmp_path / "shared.json", variant_path=tmp_path / "manager.json",
                               editor_settings_path=tmp_path / "absent-editor.json", variant_id="mod_manager")
    assert not settings.setup_complete
    captured = []
    monkeypatch.setattr(entry, "ToolkitSettings", lambda **kwargs: settings)
    monkeypatch.setattr(entry, "_set_taskbar_identity", lambda _: None)
    monkeypatch.setattr("creation_lib.mod_manager.nxm_instances.forward_link", lambda *_: False)
    monkeypatch.setattr("ui.toolkit.app_paths.get_ini_dir", lambda: tmp_path / "inbox")
    monkeypatch.setattr("ui.toolkit.setup_wizard.SetupWizard", lambda _: pytest.fail("Manager entered authoring setup"))
    def host(workspaces, configured, *, launch_path, app_variant):
        captured.append((workspaces, configured, launch_path, app_variant))
        return SimpleNamespace(run=lambda: captured.append("ran"))
    monkeypatch.setattr(entry, "ToolkitApp", host)
    entry.run_toolkit_variant("mod_manager", launch_path)
    workspaces, configured, path, variant = captured[0]
    try:
        assert captured[1] == "ran" and [workspace.id for workspace in workspaces] == ["mod_manager"]
        assert isinstance(workspaces[0], ModManagerWorkspace)
        assert configured.active_workspace == "mod_manager" and not configured.setup_complete and path == launch_path
        assert not settings.shared_path.exists()  # Standalone intake does not complete another variant's wizard.
        settings.save()
        full_settings = ToolkitSettings(shared_path=settings.shared_path, variant_path=tmp_path / "full.json",
                                        editor_settings_path=tmp_path / "absent-editor.json", variant_id="full")
        assert not full_settings.setup_complete
        assert not entry._prepare_first_run_settings(full_settings, get_variant("full"))
        assert variant is get_variant("mod_manager") and not variant.include_index_settings
        assert configured.get_game_paths("fo4")["root_dir"] == ""  # No game guessing/extraction.
    finally:
        for workspace in workspaces:
            workspace.cleanup()


def test_pending_package_waits_for_profile_and_active_worker_without_installing(tmp_path):
    from .test_save_transfer import owned
    workspace = ModManagerWorkspace()
    path = make_package(tmp_path)
    try:
        workspace.open_file(str(path))
        workspace.draw()
        assert workspace._incoming_package == str(path) and not workspace.manager.busy
        library, profile, _, _ = owned.__wrapped__(tmp_path / "local")
        m = workspace.manager
        m.library, m.profile_id = library, profile
        m.refresh()
        archive_intake.poll(workspace)
        assert m.busy and not workspace._incoming_package
        m._future.result(timeout=15)
        m.poll()
        assert m.prepared and m.prepared.package.path == path
        assert not library.rows(profile) and not any((library.root / "mods").iterdir())
    finally:
        workspace.cleanup()


def test_multiple_and_invalid_drops_preserve_pending_package(tmp_path):
    workspace = ModManagerWorkspace()
    package = make_package(tmp_path)
    try:
        assert workspace.handle_file_drop([str(package)])
        selected = workspace._incoming_package
        assert not workspace.handle_file_drop([str(package), str(package)])
        assert not workspace.handle_file_drop([str(tmp_path / "absent.zip")])
        assert workspace._incoming_package == selected and not workspace.manager.busy
        with pytest.raises(ManagerError, match="already pending"):
            workspace.open_file(package)
        wrong = tmp_path / "file.exe"
        wrong.write_bytes(b"not an archive")
        with pytest.raises(ManagerError):
            validate_intake(wrong)
    finally:
        workspace.cleanup()


def test_other_editor_startup_routes_are_preserved(tmp_path):
    assert ToolkitApp._resolve_launch_target(str(tmp_path / "Fixture.esp"))[0] == "esp_editor"
    assert ToolkitApp._resolve_launch_target(str(tmp_path / "Fixture.ba2"))[0] == "bsa_viewer"
    assert ToolkitApp._resolve_launch_target(str(tmp_path / "Fixture.nif"))[0] == "nif"
