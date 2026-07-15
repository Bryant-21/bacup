"""DDS Resizer tool — batch resize DDS textures through the native DDS backend."""

from __future__ import annotations

import logging
import os

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, draw_text_field, draw_combo_field

_log = logging.getLogger("tools.dds_resizer")

DOWNSCALE_METHODS = ["lanczos", "bicubic", "bilinear", "nearest", "box", "hamming"]


class DDSResizerTool(BaseTool):
    name = "DDS Resizer"
    tool_id = "dds_resizer"
    description = "Batch resize DDS textures"
    category = "DDS"

    def __init__(self):
        super().__init__()
        self._src_dir = ""
        self._out_dir = ""
        self._sizes_csv = "512,1024,2048"
        self._ignore_csv = ""
        self._per_size_subfolders = True
        self._no_upscale = True
        self._generate_mips = True
        self._bc3_convert = False
        self._method_idx = 0

    def _parse_sizes(self) -> list[int]:
        out = []
        for tok in self._sizes_csv.replace(";", ",").split(","):
            tok = tok.strip()
            if tok.isdigit():
                n = int(tok)
                if 1 <= n <= 16384 and n not in out:
                    out.append(n)
        return sorted(out)

    def draw_content(self) -> None:
        if begin_form("##resizer"):
            _, clicked = draw_path_row("Source", self._src_dir)
            if clicked:
                path = pick_folder("Select source folder with DDS files")
                if path:
                    self._src_dir = path
                    if not self._out_dir:
                        base = os.path.basename(os.path.normpath(path))
                        self._out_dir = os.path.join(os.path.dirname(path), base + "_resized")

            _, clicked = draw_path_row("Output", self._out_dir)
            if clicked:
                path = pick_folder("Select output folder")
                if path:
                    self._out_dir = path

            _, self._sizes_csv = draw_text_field("Sizes (CSV)", self._sizes_csv)
            _, self._ignore_csv = draw_text_field("Ignore (CSV)", self._ignore_csv)
            _, self._method_idx = draw_combo_field("Method", DOWNSCALE_METHODS, self._method_idx)
            end_form()

        imgui.text_disabled("  Ignore: subfolder patterns to skip, e.g. temp, *_bak")
        imgui.separator()

        _, self._per_size_subfolders = imgui.checkbox("Per-size subfolders", self._per_size_subfolders)
        _, self._no_upscale = imgui.checkbox("No upscale (copy if smaller)", self._no_upscale)
        _, self._generate_mips = imgui.checkbox("Generate mipmaps", self._generate_mips)
        _, self._bc3_convert = imgui.checkbox("Convert BC7 to BC3", self._bc3_convert)

        imgui.spacing()
        imgui.separator()

        if not self._running:
            if imgui.button("Run", imgui.ImVec2(120, 0)):
                self._validate_and_run()
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

    def _validate_and_run(self):
        if not self._src_dir or not os.path.isdir(self._src_dir):
            self._error_msg = "Please select a valid source folder."
            return
        if not self._out_dir:
            self._error_msg = "Please select an output folder."
            return
        sizes = self._parse_sizes()
        if not sizes:
            self._error_msg = "Please enter at least one valid size."
            return
        if not self._per_size_subfolders and len(sizes) > 1:
            self._error_msg = "Without per-size subfolders, use only one size to avoid overwrites."
            return
        self._start_batch(self._run_resize)

    def _run_resize(self):
        from creation_lib.dds import batch_resize

        sizes = self._parse_sizes()
        ignore = [p.strip() for p in self._ignore_csv.split(",") if p.strip()] if self._ignore_csv else []

        os.makedirs(self._out_dir, exist_ok=True)

        result = batch_resize(
            input_dir=self._src_dir,
            output_dir=self._out_dir,
            sizes=sizes,
            generate_mips=self._generate_mips,
            no_upscale=self._no_upscale,
            per_size_subfolders=self._per_size_subfolders,
            bc3_convert=self._bc3_convert,
            downscale_method=DOWNSCALE_METHODS[self._method_idx],
            ignore_patterns=ignore,
            progress_callback=self._on_progress,
            cancel_check=lambda: self._cancel_requested,
        )

        self._result_msg = (
            f"Done: {result['processed']} processed, {result['failed']} failed\n"
            f"Output: {self._out_dir}"
        )

    def get_default_settings(self) -> dict:
        return {
            "sizes_csv": "512,1024,2048",
            "per_size_subfolders": True,
            "no_upscale": True,
            "generate_mips": True,
            "bc3_convert": False,
            "method_idx": 0,
        }

    def apply_settings(self, settings: dict) -> None:
        self._sizes_csv = settings.get("sizes_csv", "512,1024,2048")
        self._per_size_subfolders = settings.get("per_size_subfolders", True)
        self._no_upscale = settings.get("no_upscale", True)
        self._generate_mips = settings.get("generate_mips", True)
        self._bc3_convert = settings.get("bc3_convert", False)
        self._method_idx = settings.get("method_idx", 0)

    def collect_settings(self) -> dict:
        return {
            "sizes_csv": self._sizes_csv,
            "per_size_subfolders": self._per_size_subfolders,
            "no_upscale": self._no_upscale,
            "generate_mips": self._generate_mips,
            "bc3_convert": self._bc3_convert,
            "method_idx": self._method_idx,
        }
