"""Voice Changer workspace — wraps ui.voice_changer for the toolkit."""
from __future__ import annotations

import logging

from creation_lib.ui.shell import BaseWorkspace, make_window

_log = logging.getLogger("toolkit.voice_changer")
_NS = "##voice_changer"


class VoiceChangerWorkspace(BaseWorkspace):
    """Workspace wrapper for the Voice Changer tool."""

    name = "Recorder"
    icon = "VCH"
    id = "voice_changer"
    user_guide_body = """
Manage voice changer presets, devices, recording sources, and output folders.
Use the panels to tune filters, volume, and batch settings before export.
"""

    def get_dockable_windows(self):
        return [
            make_window(f"Recorder{_NS}", "MainDockSpace"),
            make_window(f"Bulk{_NS}", "MainDockSpace"),
            make_window(f"Filter Builder{_NS}", "BottomDock"),
            make_window(f"Presets{_NS}", "LeftDock"),
            make_window(f"Recordings{_NS}", "RightDock"),
        ]

    def get_required_addons(self) -> dict:
        return {"with_implot": True}

    def initialize(self) -> None:
        from ui.voice_changer.voice_changer_app import VoiceChangerApp

        self._app = VoiceChangerApp(toolkit_settings=self._toolkit_settings)
        self._app.setup()
        self._app._init_panels()
        self._app._first_frame = False
        self._initialized = True

        if self._pending_settings:
            self._apply_saved_settings(self._pending_settings)
            self._pending_settings = None

        self._bind_panels({
            f"Recorder{_NS}": self._app.recorder_panel.draw,
            f"Bulk{_NS}": self._app.bulk_panel.draw,
            f"Filter Builder{_NS}": self._app.filter_builder_panel.draw,
            f"Presets{_NS}": self._app.preset_panel.draw,
            f"Recordings{_NS}": self._app.recordings_panel.draw,
        })
        _log.info("Voice Changer workspace initialized")

    def draw_menu(self) -> None:
        if self._view_helper:
            self._view_helper.draw([
                f"Recorder{_NS}", f"Bulk{_NS}",
                f"Filter Builder{_NS}", f"Presets{_NS}",
                f"Recordings{_NS}",
            ])

    def draw(self) -> None:
        if not self.active or not self._initialized:
            return

    def on_activate(self) -> None:
        super().on_activate()
        _log.info("Voice Changer workspace activated")

    def on_deactivate(self) -> None:
        super().on_deactivate()
        _log.info("Voice Changer workspace deactivated")

    def cleanup(self) -> None:
        if self._app and self._app.filter_builder_panel:
            self._app.filter_builder_panel.cleanup()

    def get_settings_defaults(self) -> dict:
        return {
            "active_preset": "",
            "active_presets": [],
            "input_device": 0,
            "output_device": 0,
            "volume": 1.0,
            "last_input_folder": "",
            "last_output_folder": "",
            "filter_builder_expanded": {},
        }

    def apply_settings(self, settings: dict) -> None:
        if self._initialized and self._app:
            self._apply_saved_settings(settings)
        else:
            self._pending_settings = settings

    def _apply_saved_settings(self, settings: dict):
        if self._app:
            if self._app.preset_panel:
                self._app.preset_panel.restore_settings(
                    active_preset=settings.get("active_preset", ""),
                    active_presets=settings.get("active_presets"),
                )
            if self._app.recorder_panel:
                self._app.recorder_panel.restore_settings(
                    input_device=settings.get("input_device", 0),
                    output_device=settings.get("output_device", 0),
                    volume=settings.get("volume", 1.0),
                )
            if self._app.bulk_panel:
                self._app.bulk_panel.restore_settings(
                    last_input_folder=settings.get("last_input_folder", ""),
                    last_output_folder=settings.get("last_output_folder", ""),
                )
            if self._app.filter_builder_panel:
                self._app.filter_builder_panel.restore_settings(
                    expanded=settings.get("filter_builder_expanded", {}),
                )

    def collect_settings(self) -> dict:
        if not self._initialized or not self._app:
            return {}
        result = self.get_settings_defaults()
        if self._app.preset_panel:
            result.update(self._app.preset_panel.collect_settings())
        if self._app.recorder_panel:
            result.update(self._app.recorder_panel.collect_settings())
        if self._app.bulk_panel:
            result.update(self._app.bulk_panel.collect_settings())
        if self._app.filter_builder_panel:
            result.update(self._app.filter_builder_panel.collect_settings())
        return result
