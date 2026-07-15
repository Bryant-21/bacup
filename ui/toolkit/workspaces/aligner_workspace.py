"""Scope Aligner workspace — wraps ui.aligner for the toolkit."""
from __future__ import annotations

import logging

from imgui_bundle import hello_imgui, imgui, icons_fontawesome_6 as fa

from creation_lib.ui.shell import BaseWorkspace, make_window
from creation_lib.ui.widgets.user_guide import UserGuide

_log = logging.getLogger("toolkit.aligner")
_NS = "##aligner"


class AlignerWorkspace(BaseWorkspace):
    """Workspace wrapper for the Scope Aligner."""

    name = "Scope Aligner"
    icon = "AIM"
    id = "aligner"

    def get_user_guide(self):
        from ui.aligner.panels.help_panel import USER_GUIDE_MARKDOWN

        return UserGuide("Scope Aligner User Guide", USER_GUIDE_MARKDOWN, "aligner_user_guide")

    def get_dockable_windows(self):
        return [
            make_window(f"Viewport{_NS}", "MainDockSpace"),
            make_window(f"Setup{_NS}", "LeftDock"),
            make_window(f"Offsets{_NS}", "LeftDockBottom"),
            make_window(f"Output{_NS}", "LeftDockBottom"),
            make_window(f"Help{_NS}", "RightDock", is_visible=False),
        ]

    def initialize(self) -> None:
        from ui.aligner.aligner_app import ScopeAlignerApp
        from ui.aligner.panels.help_panel import HelpPanel

        self._app = ScopeAlignerApp(toolkit_settings=self._toolkit_settings)
        self._app.setup()
        self._app._init_panels()
        self._app._first_frame = False
        self._help_panel = HelpPanel()
        self._initialized = True

        if self._pending_settings:
            self._apply_saved_paths(self._pending_settings)
            self._pending_settings = None

        self._bind_panels({
            f"Viewport{_NS}": self._app.viewport_panel.draw,
            f"Setup{_NS}": self._app.setup_panel.draw,
            f"Offsets{_NS}": self._app.offset_panel.draw,
            f"Output{_NS}": self._app.output_panel.draw,
            f"Help{_NS}": self._help_panel.draw,
        })
        _log.info("Scope Aligner workspace initialized")

    def draw_menu(self) -> None:
        if self._view_helper:
            self._view_helper.draw([
                "Viewport##aligner", "Setup##aligner",
                "Offsets##aligner", "Output##aligner",
                f"Help{_NS}",
            ])

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        def _btn(icon: str) -> bool:
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(icon)
            if icon_font:
                imgui.pop_font()
            return clicked

        has_mesh = bool(self._app and self._app.skinned_meshes)
        if has_mesh:
            imgui.push_style_color(
                imgui.Col_.button, imgui.ImVec4(0.2, 0.5, 0.2, 1.0))
        if _btn(fa.ICON_FA_PERSON):
            if self._app and self._app.viewport_panel:
                self._app.viewport_panel._show_preset_popup = True
        if has_mesh:
            imgui.pop_style_color()
        tooltip = "Load Preset Body"
        if has_mesh and self._app:
            tooltip = f"Body loaded ({len(self._app.skinned_meshes)} meshes)"
        imgui.set_item_tooltip(tooltip)

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

    def on_activate(self) -> None:
        super().on_activate()
        _log.info("Scope Aligner workspace activated")

    def on_deactivate(self) -> None:
        super().on_deactivate()
        _log.info("Scope Aligner workspace deactivated")

    def get_settings_defaults(self) -> dict:
        return {
            "weapon_path": "",
            "scope_path": "",
            "anim_path": "",
        }

    def apply_settings(self, settings: dict) -> None:
        if self._initialized and self._app:
            self._apply_saved_paths(settings)
        else:
            self._pending_settings = settings

    def _apply_saved_paths(self, settings: dict):
        if self._app and self._app.setup_panel:
            self._app.setup_panel.restore_paths(
                weapon_path=settings.get("weapon_path", ""),
                scope_path=settings.get("scope_path", ""),
                anim_path=settings.get("anim_path", ""),
            )

    def collect_settings(self) -> dict:
        if self._app and self._app.setup_panel:
            return self._app.setup_panel.collect_paths()
        return self.get_settings_defaults()
