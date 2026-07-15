"""Output log panel — displays processing status and errors."""
from __future__ import annotations

from imgui_bundle import imgui

from ui.core.imgui_widgets import draw_output_log

_NS = "##voice_changer"


class OutputLogPanel:
    """Scrollable log viewer for voice changer operations."""

    def __init__(self, app):
        self._app = app
        self.window_name = f"Log{_NS}"

    def draw(self):
        imgui.begin(self.window_name)
        draw_output_log(self._app.log_lines, max_lines=5000, label=f"##vc_log")
        imgui.end()
