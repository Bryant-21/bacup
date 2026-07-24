"""Tests for toolbar protocol compliance across all workspaces."""
from unittest.mock import MagicMock, patch


def _make_nif_workspace():
    from ui.toolkit.workspaces.nif_workspace import NifWorkspace
    ws = NifWorkspace.__new__(NifWorkspace)
    ws._app = MagicMock()
    ws._app.undo_manager = MagicMock()
    ws._app.undo_manager.can_undo = False
    ws._app.undo_manager.can_redo = False
    return ws


def _make_behavior_workspace():
    from ui.toolkit.workspaces.behavior_workspace import BehaviorWorkspace
    ws = BehaviorWorkspace.__new__(BehaviorWorkspace)
    ws._app = MagicMock()
    return ws


def _make_papyrus_workspace():
    from ui.toolkit.workspaces.papyrus_workspace import PapyrusWorkspace
    ws = PapyrusWorkspace.__new__(PapyrusWorkspace)
    ws._app = MagicMock()
    ws._app.active_path = None
    return ws


def test_nif_workspace_has_toolbar():
    ws = _make_nif_workspace()
    assert ws.has_toolbar() is True


def test_behavior_workspace_has_toolbar():
    ws = _make_behavior_workspace()
    assert ws.has_toolbar() is True


def test_papyrus_workspace_has_toolbar():
    ws = _make_papyrus_workspace()
    assert ws.has_toolbar() is True


def test_workspaces_without_toolbar_use_getattr_fallback():
    """Workspaces that don't override has_toolbar return False via getattr fallback."""
    class _NoToolbarWorkspace:
        pass

    ws = _NoToolbarWorkspace()
    result = getattr(ws, 'has_toolbar', lambda: False)()
    assert result is False


def test_draw_toolbar_callable_nif():
    ws = _make_nif_workspace()
    assert callable(ws.draw_toolbar)


def test_draw_toolbar_callable_behavior():
    ws = _make_behavior_workspace()
    assert callable(ws.draw_toolbar)


def test_draw_toolbar_callable_papyrus():
    ws = _make_papyrus_workspace()
    assert callable(ws.draw_toolbar)


def test_base_workspace_without_body_has_no_user_guide():
    from creation_lib.ui.shell import BaseWorkspace

    ws = BaseWorkspace()

    assert ws.get_user_guide() is None


def test_base_workspace_with_body_toggles_generic_guide(monkeypatch):
    from creation_lib.ui.shell import BaseWorkspace, base_workspace

    class GuideWorkspace(BaseWorkspace):
        name = "Example"
        id = "example"
        user_guide_body = "Use this workspace to test guide behavior."

    ws = GuideWorkspace()

    guide = ws.get_user_guide()
    assert guide.title == "Example User Guide"
    assert guide.window_id == "user_guide_example"
    assert ws._show_user_guide is False
    # toggle_user_guide() calls hello_imgui.get_runner_params(), which raises
    # against the real imgui_bundle without a live ImGui runner.
    monkeypatch.setattr(base_workspace, "hello_imgui", MagicMock())
    ws.toggle_user_guide()
    assert ws._show_user_guide is True


def test_base_workspace_dedents_multiline_user_guide_body():
    from creation_lib.ui.shell import BaseWorkspace

    class GuideWorkspace(BaseWorkspace):
        name = "Example"
        id = "example"
        user_guide_body = """
        # Example

        Use this workspace to test guide formatting.
        """

    guide = GuideWorkspace().get_user_guide()

    assert guide.body == "# Example\n\nUse this workspace to test guide formatting."


def test_base_workspace_exact_identifiers_do_not_fallback():
    from creation_lib.ui.shell import BaseWorkspace

    class GuideWorkspace(BaseWorkspace):
        user_guide_body = "Use this workspace to test guide behavior."

    ws = GuideWorkspace()
    guide = ws.get_user_guide()

    assert guide.title == " User Guide"
    assert guide.window_id == "user_guide_"


def test_tool_workspace_uses_tool_metadata_for_default_guide():
    from ui.toolkit.workspaces.tool_workspace import ToolWorkspace
    from ui.tools.base import BaseTool

    class ExampleTool(BaseTool):
        name = "DDS Inspector"
        tool_id = "dds_inspector"
        description = "View DDS file properties"
        category = "Textures"

    class ExampleWorkspace(ToolWorkspace):
        name = "DDS Inspector"
        icon = "DDS"
        id = "dds_inspector"
        tool_class = ExampleTool

    ws = ExampleWorkspace()
    guide = ws.get_user_guide()

    assert guide.title == "DDS Inspector User Guide"
    assert guide.window_id == "user_guide_dds_inspector"
    assert "View DDS file properties" in guide.body
    assert "## Quick Start" in guide.body
    assert "Fill in the required fields" in guide.body
    assert "Choose the input and output paths" in guide.body
    assert "Run the tool and watch the status area" in guide.body
    assert "## Output" in guide.body
    assert "Results, warnings, and errors appear below the tool controls." in guide.body
