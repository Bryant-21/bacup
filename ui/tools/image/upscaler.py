"""Image Upscaler tool — AI upscaling via chaiNNer CLI."""

from __future__ import annotations

import logging
import os
import subprocess

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, draw_text_field, draw_int_field, pick_file

_log = logging.getLogger("tools.upscaler")


class ImageUpscalerTool(BaseTool):
    name = "Image Upscaler"
    tool_id = "image_upscaler"
    description = "AI upscale via chaiNNer"
    category = "DDS"

    def __init__(self):
        super().__init__()
        self._input_path = ""
        self._output_dir = ""
        self._chainner_path = ""
        self._model_name = ""
        self._scale = 2

    def draw_content(self) -> None:
        if begin_form("##upscaler"):
            _, clicked = draw_path_row("Input", self._input_path)
            if clicked:
                path = pick_folder("Select folder with images")
                if not path:
                    path = pick_file("Select image file", [("Images", "*.png *.jpg *.dds"), ("All", "*.*")])
                if path:
                    self._input_path = path

            _, clicked = draw_path_row("Output", self._output_dir)
            if clicked:
                path = pick_folder("Select output folder")
                if path:
                    self._output_dir = path

            _, clicked = draw_path_row("chaiNNer", self._chainner_path)
            if clicked:
                path = pick_file("Select chaiNNer executable", [("Executable", "*.exe"), ("All", "*.*")])
                if path:
                    self._chainner_path = path

            _, self._model_name = draw_text_field("Model", self._model_name)
            imgui.text_disabled("  e.g. 4x_NMKD-Siax_200k, 2x_ESRGAN")

            _, self._scale = draw_int_field("Scale", self._scale, min_val=1, max_val=8)
            end_form()

        imgui.spacing()
        imgui.separator()

        if not self._running:
            if imgui.button("Run", imgui.ImVec2(120, 0)):
                if not self._input_path:
                    self._error_msg = "Please select an input path."
                    return
                if not self._output_dir:
                    self._error_msg = "Please select an output folder."
                    return
                self._start_batch(self._run_upscale)
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

        imgui.spacing()
        imgui.text_disabled("Requires chaiNNer CLI installed separately.")

    def _run_upscale(self):
        if not self._chainner_path or not os.path.isfile(self._chainner_path):
            self._error_msg = "chaiNNer executable not found. Please set the path."
            return

        os.makedirs(self._output_dir, exist_ok=True)

        # Collect input files
        files = []
        if os.path.isfile(self._input_path):
            files.append(self._input_path)
        else:
            for root, _dirs, fnames in os.walk(self._input_path):
                for f in fnames:
                    if f.lower().endswith((".png", ".jpg", ".jpeg", ".bmp", ".tga")):
                        files.append(os.path.join(root, f))

        total = len(files)
        if total == 0:
            self._result_msg = "No image files found."
            return

        processed = 0
        failed = 0

        for i, src in enumerate(files):
            if self._cancel_requested:
                break

            self._on_progress(i, total, f"Upscaling: {os.path.basename(src)}")

            out_path = os.path.join(self._output_dir, os.path.basename(src))

            try:
                cmd = [
                    self._chainner_path,
                    "--input", src,
                    "--output", out_path,
                ]
                if self._model_name:
                    cmd.extend(["--model", self._model_name])
                cmd.extend(["--scale", str(self._scale)])

                result = subprocess.run(
                    cmd,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                    timeout=300,
                )
                if result.returncode == 0:
                    processed += 1
                else:
                    _log.error("Upscale failed for %s: %s", src, result.stdout)
                    failed += 1
            except Exception as e:
                _log.exception("Upscale crashed for %s", src)
                failed += 1

        self._on_progress(total, total, "Done")
        self._result_msg = f"Done: {processed} upscaled, {failed} failed"

    def get_default_settings(self) -> dict:
        return {"chainner_path": "", "model_name": "", "scale": 2}

    def apply_settings(self, settings: dict) -> None:
        self._chainner_path = settings.get("chainner_path", "")
        self._model_name = settings.get("model_name", "")
        self._scale = settings.get("scale", 2)

    def collect_settings(self) -> dict:
        return {
            "chainner_path": self._chainner_path,
            "model_name": self._model_name,
            "scale": self._scale,
        }
