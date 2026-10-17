"""Reviewed preferences and direct header gestures without ImGui tables."""
from copy import deepcopy

from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import action_button, scaled

from creation_lib.mod_manager import columns


class ColumnsUI:
    def __init__(self, workspace):
        self.workspace = workspace
        self.draft = None
        self.expected, self.error = None, ""

    def open(self):
        self.draft = deepcopy(columns.normalize(self.workspace._columns))
        self.expected, self.error = deepcopy(self.draft), ""
        self.workspace._dialog = "Columns"
        self.workspace._tab = "Columns"

    def draw(self):
        w = self.workspace
        if self.draft is None:
            self.open()
        imgui.text_wrapped("Arrange and size the table-free mod tree. Customized columns scroll horizontally when they do not fit. Save applies this draft; Cancel discards it.")
        if self.error:
            imgui.text_wrapped(self.error)
        imgui.begin_child("column_preferences", (0, -imgui.get_frame_height_with_spacing() * 2))
        try:
            for index, key in enumerate(tuple(self.draft["order"])):
                imgui.push_id(key)
                imgui.text_unformatted(columns.LABELS[key])
                imgui.same_line()
                if action_button("Move up", enabled=index > 0):
                    self.draft = columns.moved(self.draft, key, self.draft["order"][index - 1])
                imgui.same_line()
                if action_button("Move down", enabled=index < len(columns.ORDER) - 1):
                    following = self.draft["order"][index + 1]
                    self.draft = columns.moved(self.draft, following, key)
                width = int(self.draft["widths"].get(key, columns.DEFAULT_WIDTHS[key]))
                imgui.set_next_item_width(-1)
                changed, width = imgui.slider_int("Width##column_width", width, columns.MINIMUMS[key], columns.MAXIMUMS[key])
                if changed:
                    self.draft = columns.resized(self.draft, key, width)
                imgui.pop_id()
        finally:
            imgui.end_child()
        if action_button("Reset draft to defaults"):
            self.draft = columns.normalize()
        from .toolbar_ui import _next_button
        _next_button("Save column preferences")
        if action_button("Save column preferences", primary=True):
            if w._columns != self.expected:
                self.error = "Column preferences changed while this draft was open. Cancel and reopen it."
            else:
                w._columns = columns.normalize(self.draft)
                w._dialog, self.draft = "", None
        _next_button("Cancel column preferences")
        if action_button("Cancel column preferences"):
            w._dialog, self.draft = "", None


def header_gestures(workspace, key, rectangle):
    if imgui.begin_drag_drop_source():
        workspace._column_drag = key
        imgui.set_drag_drop_payload_py_id("MODBOX_COLUMN", 1)
        imgui.text_unformatted("Move " + columns.LABELS[key])
        imgui.end_drag_drop_source()
    if imgui.begin_drag_drop_target():
        if imgui.accept_drag_drop_payload_py_id("MODBOX_COLUMN") and workspace._column_drag:
            workspace._columns = columns.moved(workspace._columns, workspace._column_drag, key)
            workspace._column_drag = None
        imgui.end_drag_drop_target()
    if imgui.begin_popup_context_item("column_actions"):
        if imgui.menu_item("Configure columns...")[0]:
            workspace.columns_ui.open()
        if imgui.menu_item("Reset column order and widths")[0]:
            workspace._columns = columns.normalize()
        imgui.end_popup()
    left, top, right, bottom = rectangle
    imgui.set_cursor_screen_pos((right - scaled(5), top))
    imgui.invisible_button("##resize_column_" + key, (scaled(5), bottom - top))
    if imgui.is_item_hovered() or imgui.is_item_active():
        imgui.set_mouse_cursor(imgui.MouseCursor_.resize_ew)
        imgui.set_item_tooltip("Drag to resize " + columns.LABELS[key])
    if imgui.is_item_active() and imgui.get_io().mouse_delta.x:
        current = workspace._column_widths[key]
        workspace._columns = columns.resized(workspace._columns, key, current + imgui.get_io().mouse_delta.x / scaled(1))
