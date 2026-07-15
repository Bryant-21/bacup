"""Image Quantizer tool — reduce image to N colors."""

from __future__ import annotations

import logging
import os

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, draw_combo_field, pick_file, pick_save_file

_log = logging.getLogger("tools.image_quantizer")

QUANT_METHODS = [
    "median_cut",
    "max_coverage",
    "fast_octree",
    "libimagequant",
    "kmeans_adaptive",
    "uniform",
]

PALETTE_SIZES = [256, 192, 128, 96, 64, 32]


class ImageQuantizerTool(BaseTool):
    name = "Image Quantizer"
    tool_id = "image_quantizer"
    description = "Reduce image to N colors"
    category = "Palettes"

    def __init__(self):
        super().__init__()
        self._image_path = ""
        self._output_path = ""
        self._method_idx = 3  # libimagequant
        self._size_idx = 2  # 128
        self._report_lines: list[str] = []

    def draw_content(self) -> None:
        if begin_form("##quantizer"):
            _, clicked = draw_path_row("Image", self._image_path)
            if clicked:
                path = pick_file(
                    "Select image",
                    [("Images", "*.png *.jpg *.jpeg *.dds *.bmp *.tga"), ("All", "*.*")],
                )
                if path:
                    self._image_path = path

            _, clicked = draw_path_row("Output", self._output_path)
            if clicked:
                path = pick_save_file(
                    "Save quantized image",
                    [("PNG", "*.png"), ("All", "*.*")],
                )
                if path:
                    self._output_path = path

            _, self._size_idx = draw_combo_field("Colors", [str(s) for s in PALETTE_SIZES], self._size_idx)
            _, self._method_idx = draw_combo_field("Method", QUANT_METHODS, self._method_idx)
            end_form()

        imgui.spacing()
        imgui.separator()

        if not self._running:
            if imgui.button("Quantize", imgui.ImVec2(120, 0)):
                self._validate_and_run()
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

        # Report
        if self._report_lines:
            imgui.spacing()
            imgui.separator()
            for line in self._report_lines:
                imgui.text(line)

    def _validate_and_run(self):
        if not self._image_path or not os.path.isfile(self._image_path):
            self._error_msg = "Please select a valid image file."
            return
        if not self._output_path:
            # Auto-generate output path
            base, ext = os.path.splitext(self._image_path)
            self._output_path = f"{base}_quantized.png"
        self._start_batch(self._run_quantize)

    def _run_quantize(self):
        import numpy as np
        from PIL import Image
        from creation_lib.dds import load_image
        from creation_lib.palette import quantize_image

        self._on_progress(0, 2, "Loading image...")

        img = load_image(self._image_path)
        if img is None:
            self._error_msg = "Failed to load image."
            return

        if self._cancel_requested:
            return

        self._on_progress(1, 2, "Quantizing...")

        method = QUANT_METHODS[self._method_idx]
        target_colors = PALETTE_SIZES[self._size_idx]
        q_img = quantize_image(img, method, target_colors)

        # Convert to RGB for analysis and saving
        rgb = q_img.convert("RGB")
        arr = np.array(rgb)
        unique = np.unique(arr.reshape(-1, 3), axis=0)
        before_colors = len(np.unique(np.array(img.convert("RGB")).reshape(-1, 3), axis=0))

        self._report_lines = [
            f"Colors: {before_colors} -> {len(unique)} (target: {target_colors})",
            f"Method: {method}",
        ]

        # Save
        os.makedirs(os.path.dirname(self._output_path) or ".", exist_ok=True)
        rgb.save(self._output_path)

        self._on_progress(2, 2, "Done")
        self._result_msg = f"Saved: {os.path.basename(self._output_path)}"

    def get_default_settings(self) -> dict:
        return {"method": "libimagequant", "palette_size": 128}

    def apply_settings(self, settings: dict) -> None:
        m = settings.get("method", "libimagequant")
        self._method_idx = QUANT_METHODS.index(m) if m in QUANT_METHODS else 3
        ps = settings.get("palette_size", 128)
        self._size_idx = PALETTE_SIZES.index(ps) if ps in PALETTE_SIZES else 2

    def collect_settings(self) -> dict:
        return {
            "method": QUANT_METHODS[self._method_idx],
            "palette_size": PALETTE_SIZES[self._size_idx],
        }
