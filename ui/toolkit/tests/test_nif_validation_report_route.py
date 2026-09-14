from __future__ import annotations

import inspect
from types import SimpleNamespace


class _StandaloneValidationWorkspace:
    name = "NIF Validation Report"
    id = "nif_validation_report"
    _initialized = True

    def __init__(self):
        self.active = False

    def on_activate(self):
        self.active = True

    def on_deactivate(self):
        self.active = False

    def draw_menu(self):
        pass


def test_workspace_nif_menu_switches_to_standalone_validation_tool(monkeypatch):
    from ui.toolkit.app import ToolkitApp

    workspace = _StandaloneValidationWorkspace()
    settings = SimpleNamespace(
        active_workspace="search",
        save=lambda: None,
    )
    app = ToolkitApp.__new__(ToolkitApp)
    app._app_variant = SimpleNamespace(is_standalone=False)
    app._active_ws = None
    app._ws_map = {workspace.id: workspace}
    app._settings = settings
    app._show_about = False
    app._show_theme_selector = False

    monkeypatch.setattr(
        "ui.toolkit.app.imgui.begin_menu",
        lambda label: label in {"Workspace", "NIF"},
    )
    monkeypatch.setattr(
        "ui.toolkit.app.imgui.menu_item",
        lambda label, *args, **kwargs: (label == workspace.name, False),
    )
    monkeypatch.setattr("ui.toolkit.app.imgui.separator", lambda: None)
    monkeypatch.setattr("ui.toolkit.app.imgui.end_menu", lambda: None)

    app._show_menus()

    assert app._active_ws is workspace
    assert workspace.active is True


def test_validation_report_is_registered_as_standalone_tool_not_editor_panel():
    from ui.toolkit.workspaces import create_workspaces
    from ui.toolkit.workspaces.nif_workspace import NifWorkspace
    from ui.tools.meshes import nif_validation_tool

    workspace = create_workspaces(workspace_ids=("nif_validation_report",))[0]

    assert workspace.id == "nif_validation_report"
    assert workspace.name == "NIF Validator"
    assert workspace.__class__.__module__ == "ui.toolkit.workspaces.nif_tools"
    assert workspace._tool.__class__.__module__ == (
        "ui.tools.meshes.nif_validation_tool"
    )
    assert "ui.editor" not in inspect.getsource(nif_validation_tool)
    assert "Validation Report" not in inspect.getsource(NifWorkspace)
