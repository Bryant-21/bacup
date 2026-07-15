"""Mod list panel — searchable dockable selector for the Mod Builder."""
from __future__ import annotations

from imgui_bundle import imgui

_NS = "##mod_builder"


class ModListPanel:
    """Left-dock mod browser with filtering and selection."""

    def __init__(self, app):
        self._app = app
        self.window_name = f"Mods{_NS}"

    def draw(self):
        visible, _ = imgui.begin(self.window_name)
        if not visible:
            imgui.end()
            return

        self._app._draw_mod_selector()
        imgui.end()
