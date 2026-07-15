"""Mod Builder workspace — thin wrapper around ui.builder.ModBuilderApp."""
from __future__ import annotations

import logging

from imgui_bundle import hello_imgui, imgui

from creation_lib.ui.shell import BaseWorkspace, make_window
from creation_lib.ui.widgets.user_guide import UserGuide

_log = logging.getLogger("toolkit.mod_builder")
_NS = "##mod_builder"


class ModBuilderWorkspace(BaseWorkspace):
    """Workspace wrapper for the Mod Builder."""

    name = "Mod Manager"
    icon = "MOD"
    id = "mod_builder"

    def get_user_guide(self):
        from ui.builder.panels.help_panel import USER_GUIDE_MARKDOWN

        return UserGuide("Mod Builder User Guide", USER_GUIDE_MARKDOWN, "mod_builder_user_guide")

    def get_dockable_windows(self):
        return [
            make_window(f"Mods{_NS}", "LeftDock"),
            make_window(f"Mod Builder{_NS}", "MainDockSpace"),
            make_window(f"Help{_NS}", "RightDock", is_visible=False),
        ]

    def initialize(self) -> None:
        from ui.builder.mod_builder_app import ModBuilderApp
        from ui.builder.panels.mod_list_panel import ModListPanel
        from ui.builder.panels.help_panel import HelpPanel

        self._app = ModBuilderApp(self._toolkit_settings)
        self._mod_list_panel = ModListPanel(self._app)
        self._help_panel = HelpPanel()
        self._bind_panels({
            f"Mods{_NS}": self._mod_list_panel.draw,
            f"Mod Builder{_NS}": lambda: self._app.draw_main(),
            f"Help{_NS}": self._help_panel.draw,
        })
        self._initialized = True
        _log.info("Mod Builder workspace initialized")

    def draw_menu(self) -> None:
        if self._view_helper:
            self._view_helper.draw([f"Mods{_NS}", "Mod Builder##mod_builder", f"Help{_NS}"])

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
        self._app.poll_runner()

    def get_settings_defaults(self) -> dict:
        return {"transcription_fallback": "none", "archive_max_size_gb": 4.0}

    def draw_settings(self) -> None:
        """Draw mod builder settings in the Settings window."""
        from imgui_bundle import imgui

        imgui.text("Release Audio")
        imgui.separator()
        imgui.spacing()

        _FALLBACK_OPTIONS = ["none", "parakeet", "whisper"]
        _FALLBACK_LABELS = [
            "None (skip WAVs without transcript)",
            "Parakeet (nvidia/parakeet-tdt-0.6b-v3)",
            "Whisper (openai-whisper)",
        ]
        ts = self._app._toolkit_settings
        ws = ts.get_workspace_settings("mod_builder") if ts else {}
        current = ws.get("transcription_fallback", "none")
        idx = _FALLBACK_OPTIONS.index(current) if current in _FALLBACK_OPTIONS else 0

        imgui.text("Transcription Fallback:")
        if imgui.is_item_hovered():
            imgui.set_tooltip(
                "When a voice WAV has no transcript in YAML, use this model\n"
                "to auto-transcribe for LIP generation."
            )
        imgui.same_line()
        imgui.push_item_width(300)
        changed, new_idx = imgui.combo("##trans_fallback", idx, _FALLBACK_LABELS)
        imgui.pop_item_width()
        if changed and ts:
            ts.set_workspace_settings("mod_builder", {
                "transcription_fallback": _FALLBACK_OPTIONS[new_idx],
            })
            ts.save()

        imgui.spacing()
        imgui.text("Archive Packing")
        imgui.separator()
        imgui.spacing()

        archive_max_size = float(ws.get("archive_max_size_gb", 4.0) or 4.0)
        imgui.text("Max Archive Size (GiB):")
        if imgui.is_item_hovered():
            imgui.set_tooltip("Archives larger than this are split during BA2/BSA packing.")
        imgui.same_line()
        imgui.push_item_width(120)
        changed, archive_max_size = imgui.input_float(
            "##archive_max_size_gb",
            archive_max_size,
            0.25,
            1.0,
            "%.2f",
        )
        imgui.pop_item_width()
        if changed and ts:
            ts.set_workspace_settings("mod_builder", {
                "archive_max_size_gb": max(0.01, archive_max_size),
            })
            ts.save()
