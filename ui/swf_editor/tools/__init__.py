"""SWF editor tools — base class for tool implementations."""
from __future__ import annotations

from abc import ABC, abstractmethod
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ui.swf_editor.swf_editor_app import SwfEditorApp


class BaseTool(ABC):
    """Base class for editor tools."""

    name: str = ""
    cursor: str = "arrow"

    def __init__(self, app: SwfEditorApp):
        self.app = app

    @abstractmethod
    def on_mouse_down(self, x: float, y: float, button: int) -> bool:
        """Handle mouse press. Returns True if handled."""
        ...

    @abstractmethod
    def on_mouse_move(self, x: float, y: float) -> None:
        """Handle mouse movement."""
        ...

    @abstractmethod
    def on_mouse_up(self, x: float, y: float, button: int) -> None:
        """Handle mouse release."""
        ...

    def on_key(self, key: int, down: bool) -> bool:
        """Handle key press. Returns True if handled."""
        return False

    def draw_overlay(self, draw_list, camera) -> None:
        """Draw tool-specific overlays (selection boxes, handles, etc.)."""
        pass
