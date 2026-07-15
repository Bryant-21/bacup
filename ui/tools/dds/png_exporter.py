"""DDS to PNG Exporter tool — batch convert DDS textures to PNG."""

from __future__ import annotations

import logging
import os

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row

_log = logging.getLogger("tools.dds_png_exporter")


class DDSPNGExporterTool(BaseTool):
    name = "DDS to PNG"
    tool_id = "dds_png_exporter"
    description = "Batch convert DDS to PNG"
    category = "DDS"

    def __init__(self):
        super().__init__()
        self._src_dir = ""
        self._out_dir = ""
        self._recurse = True

    def draw_content(self) -> None:
        if begin_form("##png_exporter"):
            _, clicked = draw_path_row("Source", self._src_dir)
            if clicked:
                path = pick_folder("Select folder with DDS files")
                if path:
                    self._src_dir = path
                    if not self._out_dir:
                        base = os.path.basename(os.path.normpath(path))
                        self._out_dir = os.path.join(os.path.dirname(path), base + "_png")

            _, clicked = draw_path_row("Output", self._out_dir)
            if clicked:
                path = pick_folder("Select output folder for PNGs")
                if path:
                    self._out_dir = path
            end_form()

        imgui.separator()
        _, self._recurse = imgui.checkbox("Include subdirectories", self._recurse)

        imgui.spacing()
        imgui.separator()

        if not self._running:
            if imgui.button("Run", imgui.ImVec2(120, 0)):
                if not self._src_dir or not os.path.isdir(self._src_dir):
                    self._error_msg = "Please select a valid source folder."
                    return
                if not self._out_dir:
                    self._error_msg = "Please select an output folder."
                    return
                self._start_batch(self._run_export)
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

    def _run_export(self):
        from creation_lib.dds import batch_to_png

        os.makedirs(self._out_dir, exist_ok=True)

        result = batch_to_png(
            input_dir=self._src_dir,
            output_dir=self._out_dir,
            recurse=self._recurse,
            progress_callback=self._on_progress,
            cancel_check=lambda: self._cancel_requested,
        )

        self._result_msg = (
            f"Done: {result['processed']} converted, {result['failed']} failed\n"
            f"Output: {self._out_dir}"
        )
