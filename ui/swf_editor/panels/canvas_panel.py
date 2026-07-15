"""Viewport panel -- renders the 2D SWF canvas via FBO.

Pan (middle-drag), zoom (scroll), tool input delegation.
"""
from __future__ import annotations

from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.swf_editor.swf_editor_app import SwfEditorApp


class CanvasPanel:
    def __init__(self, app: SwfEditorApp):
        self.app = app

    def draw(self) -> None:
        """Render the viewport panel -- delegates to app._draw_viewport()."""
        self.app._draw_viewport()
