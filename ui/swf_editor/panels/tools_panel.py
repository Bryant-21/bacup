"""Tool selector sidebar -- vertical icon strip."""
from __future__ import annotations

from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.swf_editor.swf_editor_app import SwfEditorApp

_TOOLS = [
    ("V", "select", "Select (V)"),
    ("A", "direct_select", "Direct Select (A)"),
    ("P", "pen", "Pen (P)"),
    ("R", "rect", "Rectangle (R)"),
    ("E", "ellipse", "Ellipse (E)"),
    ("L", "line", "Line (L)"),
    ("G", "fill", "Fill Bucket (G)"),
    ("I", "eyedropper", "Eyedropper (I)"),
    ("H", "hand", "Hand (H)"),
    ("Z", "zoom", "Zoom (Z)"),
]


class ToolsPanel:
    def __init__(self, app: SwfEditorApp):
        self.app = app

    def draw(self) -> None:
        visible, _ = imgui.begin("Tools##swf")
        if not visible:
            imgui.end()
            return

        for icon, tool_id, tooltip in _TOOLS:
            is_active = self.app.active_tool == tool_id
            if is_active:
                imgui.push_style_color(imgui.Col_.button, imgui.ImVec4(0.3, 0.5, 0.8, 1.0))

            if imgui.button(icon, imgui.ImVec2(32, 32)):
                self.app.active_tool = tool_id

            if is_active:
                imgui.pop_style_color()

            if imgui.is_item_hovered():
                imgui.set_tooltip(tooltip)

        imgui.end()
