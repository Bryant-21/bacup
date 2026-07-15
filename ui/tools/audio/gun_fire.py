"""Gun Fire Generator tool — generate automatic fire sound sequences."""

from __future__ import annotations

import logging

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import (
    begin_form, end_form, draw_path_row, draw_run_cancel_buttons,
    draw_text_field, draw_int_field, draw_float_field, pick_file,
)

_log = logging.getLogger("tools.gun_fire")


class GunFireTool(BaseTool):
    name = "Gun Fire Generator"
    tool_id = "gun_fire"
    description = "Generate auto-fire sound files"
    category = "Audio"

    def __init__(self):
        super().__init__()
        self._source_wav = ""
        self._output_dir = ""
        self._rpms_csv = "300, 450, 540, 660, 780, 900"
        self._shot_count = 12
        self._tail_threshold = -35.0
        self._pitch_variation = 0.5
        self._gain_variation = 2.0
        self._jitter_ms = 8
        self._highpass_enabled = True
        self._tilt_amount = 0.0
        self._base_reinforcement = False
        self._shot_variants_enabled = False
        self._shot_variant_count = 8
        self._early_reflections_enabled = False
        self._tone_color_enabled = False

    def draw_content(self) -> None:
        if begin_form("##gun_fire"):
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

            _, self._rpms_csv = draw_text_field("RPMs (CSV)", self._rpms_csv)
            _, self._shot_count = draw_int_field("Shot Count", self._shot_count, min_val=1, max_val=999)
            _, self._tail_threshold = draw_float_field("Tail Threshold (dB)", self._tail_threshold, 1.0, 5.0, "%.1f")
            _, self._pitch_variation = draw_float_field("Pitch Variation", self._pitch_variation, 0.1, 0.5, "%.2f")
            _, self._gain_variation = draw_float_field("Gain Variation (dB)", self._gain_variation, 0.1, 0.5, "%.2f")
            _, self._jitter_ms = draw_int_field("Jitter (ms)", self._jitter_ms, min_val=0)
            _, self._tilt_amount = draw_float_field("Tilt EQ", self._tilt_amount, 0.1, 0.5, "%.2f")
            end_form()

        imgui.separator()
        _, self._highpass_enabled = imgui.checkbox("Random Highpass Filter", self._highpass_enabled)
        _, self._base_reinforcement = imgui.checkbox("Bass Reinforcement", self._base_reinforcement)
        _, self._shot_variants_enabled = imgui.checkbox("Shot Variant Pool", self._shot_variants_enabled)
        if self._shot_variants_enabled:
            imgui.same_line()
            imgui.set_next_item_width(100)
            _, self._shot_variant_count = imgui.input_int("##shot_variant_count", self._shot_variant_count, 1, 4)
            self._shot_variant_count = max(2, min(self._shot_variant_count, 32))
        _, self._early_reflections_enabled = imgui.checkbox("Early Reflections", self._early_reflections_enabled)
        _, self._tone_color_enabled = imgui.checkbox("Tonal Color", self._tone_color_enabled)

        imgui.spacing()
        imgui.separator()

        can_run = bool(self._source_wav and self._output_dir and self._rpms_csv.strip())
        run_clicked, cancel_clicked = draw_run_cancel_buttons(self._running, can_run)
        if run_clicked:
            self._run_generation()
        if cancel_clicked:
            self._cancel_requested = True

    def _parse_rpms(self) -> list[int]:
        rpms = []
        for tok in self._rpms_csv.replace(";", ",").split(","):
            tok = tok.strip()
            if tok.isdigit():
                rpms.append(int(tok))
        return rpms

    def _run_generation(self) -> None:
        rpms = self._parse_rpms()
        if not rpms:
            self._error_msg = "No valid RPM values provided."
            return

        self._start_batch(
            self._do_generate,
            self._source_wav,
            self._output_dir,
            rpms,
        )

    def _do_generate(self, source_wav: str, output_dir: str, rpms: list[int]) -> None:
        from creation_lib.audio import generate_gun_fire

        result = generate_gun_fire(
            source_wav=source_wav,
            output_dir=output_dir,
            rpms=rpms,
            shot_count=self._shot_count,
            tail_threshold=self._tail_threshold,
            pitch_variation=self._pitch_variation,
            gain_variation=self._gain_variation,
            jitter_ms=self._jitter_ms,
            highpass_enabled=self._highpass_enabled,
            tilt_amount=self._tilt_amount,
            base_reinforcement=self._base_reinforcement,
            shot_variant_count=self._shot_variant_count if self._shot_variants_enabled else 0,
            early_reflections_enabled=self._early_reflections_enabled,
            tone_color_enabled=self._tone_color_enabled,
            progress_callback=self._on_progress,
            cancel_check=lambda: self._cancel_requested,
        )

        n_files = len(result.get("files", []))
        n_errors = len(result.get("errors", []))
        if n_errors:
            self._error_msg = f"{n_errors} error(s): " + "; ".join(result["errors"][:3])
        self._result_msg = f"Generated {n_files} file(s) for {len(rpms)} RPM value(s)."
