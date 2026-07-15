"""Palette Texture workspace — remap + gradient texture generator."""
from __future__ import annotations

import logging

from imgui_bundle import hello_imgui, imgui, icons_fontawesome_6 as fa

from creation_lib.ui.shell import BaseWorkspace, make_window
from creation_lib.ui.widgets.user_guide import UserGuide

_log = logging.getLogger("toolkit.palette")
_NS = "##palette_ws"


class PaletteWorkspace(BaseWorkspace):
    """Workspace for generating FO4 remap + gradient textures."""

    name = "Palette"
    icon = "PAL"
    id = "palette"

    def get_user_guide(self):
        from ui.palette.panels.help_panel import USER_GUIDE_MARKDOWN

        return UserGuide(
            "Palette Texture Generator User Guide",
            USER_GUIDE_MARKDOWN,
            "palette_user_guide",
        )

    def get_dockable_windows(self):
        return [
            make_window(f"Palette{_NS}", "MainDockSpace"),
            make_window(f"Help{_NS}", "RightDock", is_visible=False),
        ]

    def initialize(self) -> None:
        from ui.palette.app import PaletteApp
        from ui.palette.panels.help_panel import HelpPanel
        self._app = PaletteApp()
        self._help_panel = HelpPanel()
        if self._toolkit_settings:
            ws = self._toolkit_settings.get_workspace_settings(self.id)
            self._app.apply_settings(ws)
        self._bind_panels({
            f"Palette{_NS}": self._draw_panel,
            f"Help{_NS}": self._help_panel.draw,
        })
        self._initialized = True
        _log.info("PaletteWorkspace initialized")

    def _draw_panel(self) -> None:
        if imgui.begin(f"Palette{_NS}"):
            if self._app:
                self._app.draw()
        imgui.end()

    def draw_menu(self) -> None:
        if self._view_helper:
            self._view_helper.draw(["Palette##palette_ws", f"Help{_NS}"])

    def _toggle_help_panel(self):
        dp = hello_imgui.get_runner_params().docking_params
        for w in dp.dockable_windows:
            if w.label == f"Help{_NS}":
                w.is_visible = not w.is_visible
                break

    def toggle_user_guide(self) -> None:
        self._toggle_help_panel()

    def draw(self) -> None:
        if not self.active or not self._initialized:
            return
        io = imgui.get_io()
        if imgui.is_key_pressed(imgui.Key.f1) and not io.want_text_input:
            self._toggle_help_panel()

    def on_deactivate(self) -> None:
        self.active = False

    def cleanup(self) -> None:
        if self._app:
            self._app.cleanup()

    def get_settings_defaults(self) -> dict:
        return {
            "last_source_path": "",
            "last_output_path": "",
            "n_zones": 6,
            "gradient_width": 32,
            "active_tab": "auto",
            "debug_preview": False,
            "gradient_banded": False,
            "paint_index": 1.0,
            "variant_remap_path": "",
            "variant_output_path": "",
        }

    def apply_settings(self, settings: dict) -> None:
        if self._app:
            self._app.apply_settings(settings)

    def collect_settings(self) -> dict:
        if self._app:
            return self._app.collect_settings()
        return self.get_settings_defaults()
