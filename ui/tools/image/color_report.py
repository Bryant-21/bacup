"""Color Report tool — analyze color usage in images/textures."""

from __future__ import annotations

import logging
import os
from collections import Counter

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, draw_int_field, pick_file

_log = logging.getLogger("tools.color_report")


class ColorReportTool(BaseTool):
    name = "Color Report"
    tool_id = "color_report"
    description = "Analyze texture color usage"
    category = "DDS"

    def __init__(self):
        super().__init__()
        self._input_path = ""
        self._recurse = True
        self._top_n = 20
        self._report_lines: list[str] = []

    def draw_content(self) -> None:
        if begin_form("##color_report"):
            _, clicked = draw_path_row("Input", self._input_path)
            if clicked:
                path = pick_folder("Select folder with images")
                if not path:
                    path = pick_file("Select image", [("Images", "*.png *.jpg *.dds *.bmp"), ("All", "*.*")])
                if path:
                    self._input_path = path
            end_form()

        _, self._recurse = imgui.checkbox("Include subdirectories", self._recurse)

        if begin_form("##color_report_opts"):
            _, self._top_n = draw_int_field("Top N colors", self._top_n, min_val=1, max_val=100)
            end_form()

        imgui.spacing()
        imgui.separator()

        if not self._running:
            if imgui.button("Analyze", imgui.ImVec2(120, 0)):
                if not self._input_path:
                    self._error_msg = "Please select an input path."
                    return
                self._report_lines = []
                self._start_batch(self._run_analysis)
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

        if self._report_lines:
            imgui.spacing()
            imgui.separator()
            imgui.text("Report:")
            for line in self._report_lines:
                imgui.text(line)

    def _run_analysis(self):
        from PIL import Image

        files = []
        if os.path.isfile(self._input_path):
            files.append(self._input_path)
        else:
            for root, dirs, fnames in os.walk(self._input_path):
                if not self._recurse:
                    dirs[:] = []
                for f in fnames:
                    if f.lower().endswith((".png", ".jpg", ".jpeg", ".bmp", ".tga")):
                        files.append(os.path.join(root, f))

        total = len(files)
        if total == 0:
            self._result_msg = "No image files found."
            return

        global_counter: Counter = Counter()
        total_pixels = 0

        for i, path in enumerate(files):
            if self._cancel_requested:
                break

            self._on_progress(i, total, f"Analyzing: {os.path.basename(path)}")

            try:
                img = Image.open(path).convert("RGB")
                # Quantize to reduce unique colors for meaningful stats
                small = img.resize((min(img.width, 256), min(img.height, 256)), Image.Resampling.NEAREST)
                pixels = list(small.getdata())
                total_pixels += len(pixels)
                global_counter.update(pixels)
                img.close()
            except Exception as e:
                _log.warning("Failed to analyze %s: %s", path, e)

        # Build report
        lines = [
            f"Files analyzed: {total}",
            f"Total pixels sampled: {total_pixels:,}",
            f"Unique colors: {len(global_counter):,}",
            "",
            f"Top {self._top_n} colors (R, G, B) — count — %:",
        ]

        for color, count in global_counter.most_common(self._top_n):
            pct = (count / total_pixels * 100) if total_pixels else 0
            r, g, b = color
            lines.append(f"  ({r:3d}, {g:3d}, {b:3d})  {count:>8,}  {pct:5.1f}%")

        self._report_lines = lines
        self._on_progress(total, total, "Done")
        self._result_msg = f"Analysis complete: {total} files, {len(global_counter):,} unique colors"
