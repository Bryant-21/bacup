"""Laser Beam Generator tool — generate looping beam sounds from single shots."""

from __future__ import annotations

import logging

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import (
    begin_form, end_form, draw_path_row, draw_run_cancel_buttons,
    draw_float_field, pick_file,
)

_log = logging.getLogger("tools.laser_beam")


class LaserBeamTool(BaseTool):
    name = "Laser Beam Generator"
    tool_id = "laser_beam"
    description = "Generate looping beam sounds"
    category = "Audio"

    def __init__(self):
        super().__init__()
        self._source_wav = ""
        self._output_dir = ""
        self._loop_duration = 2.0
        self._pitch_variation = 0.3
        self._gain_variation = 0.5
        self._tail_threshold = -40.0
        self._highpass_enabled = False
        self._highpass_cutoff = 200.0
        self._tilt_amount = 0.0

    def draw_content(self) -> None:
        if begin_form("##laser_beam"):
            _, clicked = draw_path_row("Source WAV", self._source_wav)
            if clicked:
                path = pick_file("Select single-shot WAV", [("WAV files", "*.wav")])
                if path:
                    self._source_wav = path
                    if not self._output_dir:
                        import os
                        self._output_dir = os.path.dirname(path)

            _, clicked = draw_path_row("Output Dir", self._output_dir)
            if clicked:
                path = pick_folder("Select output directory")
                if path:
                    self._output_dir = path

            _, self._loop_duration = draw_float_field("Loop Duration (s)", self._loop_duration, 0.1, 0.5, "%.2f")
            _, self._pitch_variation = draw_float_field("Pitch Variation", self._pitch_variation, 0.1, 0.5, "%.2f")
            _, self._gain_variation = draw_float_field("Gain Variation (dB)", self._gain_variation, 0.1, 0.5, "%.2f")
            _, self._tail_threshold = draw_float_field("Tail Threshold (dB)", self._tail_threshold, 1.0, 5.0, "%.1f")
            _, self._tilt_amount = draw_float_field("Tilt EQ", self._tilt_amount, 0.1, 0.5, "%.2f")
            end_form()

        imgui.separator()
        _, self._highpass_enabled = imgui.checkbox("Highpass Filter", self._highpass_enabled)
        if self._highpass_enabled:
            imgui.same_line()
            imgui.set_next_item_width(120)
            _, self._highpass_cutoff = imgui.input_float("##hp_cutoff", self._highpass_cutoff, 10.0, 50.0, "%.0f Hz")

        imgui.spacing()
        imgui.separator()

        can_run = bool(self._source_wav and self._output_dir)
        run_clicked, cancel_clicked = draw_run_cancel_buttons(self._running, can_run)
        if run_clicked:
            self._start_batch(self._do_generate, self._source_wav, self._output_dir)
        if cancel_clicked:
            self._cancel_requested = True

    def _do_generate(self, source_wav: str, output_dir: str) -> None:
        from creation_lib.audio import generate_laser_beam

        result = generate_laser_beam(
            source_wav=source_wav,
            output_dir=output_dir,
            loop_duration=self._loop_duration,
            pitch_variation=self._pitch_variation,
            gain_variation=self._gain_variation,
            tail_threshold=self._tail_threshold,
            highpass_enabled=self._highpass_enabled,
            highpass_cutoff=self._highpass_cutoff,
            tilt_amount=self._tilt_amount,
            progress_callback=self._on_progress,
            cancel_check=lambda: self._cancel_requested,
        )

        n_files = len(result.get("files", []))
        n_errors = len(result.get("errors", []))
        if n_errors:
            self._error_msg = "; ".join(result["errors"][:3])
        self._result_msg = f"Generated {n_files} beam sound file(s)."
