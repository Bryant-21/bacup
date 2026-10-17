"""Actual persisted preference gestures and pane-scoped command input."""
from copy import deepcopy

from imgui_bundle import imgui
import pytest

from creation_lib.mod_manager import columns
from . import test_widget_ids as widgets

native, workspace = widgets.native, widgets.workspace


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.0), ((1600, 900), 1.25), ((1280, 800), 1.5)])
def test_column_draft_cancel_save_resize_and_shortcut_input(native, workspace, size, scale):
    native.io.display_size = size
    imgui.get_style().font_size_base = 16
    imgui.get_style().font_scale_main = scale
    workspace._show_filters = False
    draw = widgets.panels(workspace)
    native.warm(draw)
    original = deepcopy(workspace._columns)
    workspace.columns_ui.open()
    native.warm(draw)
    workspace.columns_ui.draft = columns.moved(workspace.columns_ui.draft, "name", "priority")
    native.click(native.find("Cancel column preferences"), draw)
    assert workspace._columns == original
    workspace.columns_ui.open()
    native.warm(draw)
    workspace.columns_ui.draft = columns.moved(workspace.columns_ui.draft, "name", "priority")
    native.click(native.find("Save column preferences"), draw)
    assert workspace._columns["order"].index("name") < workspace._columns["order"].index("priority")
    # A direct header grip resizes, while sort/header IDs remain distinct.
    workspace._columns = columns.normalize()
    native.warm(draw)
    left, top, right, bottom = workspace._column_header_rects["priority"]
    start = (right - 2, (top + bottom) / 2)
    native.frame(draw, mouse=start, down=True)
    native.frame(draw, mouse=(start[0] + 40, start[1]), down=True)
    native.frame(draw, down=False)
    assert workspace._columns["widths"]["priority"] > 76
    saved = workspace.collect_settings()
    workspace._columns = columns.normalize()
    workspace.apply_settings(saved)
    assert workspace._columns == saved["columns"]
    workspace._restore_library = False
    native.warm(draw)
    workspace.shortcuts_ui.open()
    native.warm(draw)
    for _ in range(12):
        if any(item.label == "##binding_refresh" for item in native.items):
            break
        native.scroll(next(item for item in native.items if item.label.startswith("##binding_")), draw, -1)
    native.text(native.find("##binding_refresh"), draw, "F6")
    native.click(native.find("Save shortcuts"), draw)
    calls = []
    refresh = workspace.manager.refresh_conflicts
    workspace.manager.refresh_conflicts = lambda **kwargs: calls.append(kwargs)
    try:
        native.click(native.find("Mod name###sort_name"), draw)
        native.key(imgui.Key.f5, draw)
        assert not calls
        native.key(imgui.Key.f6, draw)
        assert calls == [{"delay": 0}]
    finally:
        workspace.manager.refresh_conflicts = refresh
    assert workspace.collect_settings()["shortcuts"]["refresh"] == "F6"


def test_actual_header_drag_reorders_without_changing_load_priorities(native, workspace):
    draw = widgets.panels(workspace)
    workspace._show_filters = False
    native.warm(draw)
    before = list(workspace.manager.rows)
    def center(key):
        left, top, right, bottom = workspace._column_header_rects[key]
        return (left + right) / 2, (top + bottom) / 2
    start, target = center("priority"), center("enabled")
    native.frame(draw, mouse=start, down=True)
    native.frame(draw, mouse=(start[0] + 12, start[1]), down=True)
    native.frame(draw, mouse=target, down=True)
    native.frame(draw, mouse=target, down=False)
    native.warm(draw)
    assert workspace._columns["order"].index("priority") < workspace._columns["order"].index("enabled")
    assert workspace.manager.rows == before


def test_stale_preference_drafts_and_conflicting_shortcuts_preserve_current_settings(native, workspace):
    draw = widgets.panels(workspace)
    native.warm(draw)
    workspace.columns_ui.open()
    workspace._columns = columns.resized(workspace._columns, "name", 500)
    native.warm(draw)
    native.click(native.find("Save column preferences"), draw)
    assert "changed" in workspace.columns_ui.error and workspace._columns["widths"]["name"] == 500
    native.click(native.find("Cancel column preferences"), draw)
    workspace.shortcuts_ui.open()
    native.warm(draw)
    workspace.shortcuts_ui.draft["mod_edit"] = "Ctrl+E"
    native.click(native.find("Save shortcuts"), draw)
    assert "conflicts" in workspace.shortcuts_ui.error and workspace._shortcuts["mod_edit"] == "F2"
    native.click(native.find("Cancel shortcuts"), draw)
    assert workspace._shortcuts["mod_edit"] == "F2"
