"""Audio Extractor tool — extract WAV from FUZ/XWM files."""

from __future__ import annotations

import logging
import os
import shutil
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor, as_completed

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, draw_run_cancel_buttons, pick_file

_log = logging.getLogger("tools.audio_extractor")


def _audio_worker_count() -> int:
    return max(1, (os.cpu_count() or 1) // 2)


class AudioExtractorTool(BaseTool):
    name = "Audio Extractor"
    tool_id = "audio_extractor"
    description = "Extract WAV from FUZ/XWM files"
    category = "Audio"

    def __init__(self):
        super().__init__()
        self._input_path = ""
        self._output_dir = ""
        self._include_subdirs = True
        self._keep_intermediate = False
        self._mode = 0  # 0 = folder, 1 = file

    def draw_content(self) -> None:
        # Mode selector
        if imgui.radio_button("Folder", self._mode == 0):
            self._mode = 0
        imgui.same_line()
        if imgui.radio_button("Single File", self._mode == 1):
            self._mode = 1

        if begin_form("##extractor"):
            if self._mode == 0:
                _, clicked = draw_path_row("Input Folder", self._input_path)
                if clicked:
                    path = pick_folder("Select folder with FUZ/XWM files")
                    if path:
                        self._input_path = path
            else:
                _, clicked = draw_path_row("Input File", self._input_path)
                if clicked:
                    path = pick_file("Select audio file", [("Audio", "*.fuz *.xwm")])
                    if path:
                        self._input_path = path

            _, clicked = draw_path_row("Output Dir", self._output_dir)
            if clicked:
                path = pick_folder("Select output directory (optional)")
                if path:
                    self._output_dir = path
            end_form()

        imgui.text_disabled("Leave empty to output next to input")

        imgui.separator()

        if self._mode == 0:
            _, self._include_subdirs = imgui.checkbox("Include subdirectories", self._include_subdirs)
        _, self._keep_intermediate = imgui.checkbox("Keep .xwm and .lip files", self._keep_intermediate)

        imgui.spacing()
        imgui.separator()

        can_run = bool(self._input_path)
        run_clicked, cancel_clicked = draw_run_cancel_buttons(self._running, can_run)
        if run_clicked:
            self._start_batch(self._do_extract)
        if cancel_clicked:
            self._cancel_requested = True

    def _do_extract(self) -> None:
        input_path = os.path.abspath(self._input_path)
        output_dir = self._output_dir or None

        # Collect files
        tasks: list[tuple[str, str]] = []  # (file_path, rel_root)

        if os.path.isfile(input_path):
            ext = os.path.splitext(input_path)[1].lower()
            if ext in (".fuz", ".xwm"):
                tasks.append((input_path, "."))
            base_dir = os.path.dirname(input_path)
        else:
            base_dir = input_path
            if self._include_subdirs:
                for root, _, files in os.walk(base_dir):
                    rel = os.path.relpath(root, base_dir)
                    for fn in files:
                        if os.path.splitext(fn)[1].lower() in (".fuz", ".xwm"):
                            tasks.append((os.path.join(root, fn), rel))
            else:
                for fn in os.listdir(base_dir):
                    full = os.path.join(base_dir, fn)
                    if os.path.isfile(full) and os.path.splitext(fn)[1].lower() in (".fuz", ".xwm"):
                        tasks.append((full, "."))

        total = len(tasks)
        if total == 0:
            self._result_msg = "No .fuz or .xwm files found."
            return

        task_groups: dict[str, list[tuple[str, str, str, str, str]]] = {}
        for src, rel in tasks:
            ext = os.path.splitext(src)[1].lower()
            base_name = os.path.splitext(os.path.basename(src))[0]

            if output_dir:
                rel_dir = rel if rel and rel != "." else ""
                target_dir = os.path.join(output_dir, rel_dir)
            else:
                target_dir = os.path.dirname(src)
            os.makedirs(target_dir, exist_ok=True)

            target_wav = os.path.join(target_dir, base_name + ".wav")
            target_key = os.path.normcase(os.path.abspath(target_wav))
            task_groups.setdefault(target_key, []).append((src, ext, target_wav, target_dir, base_name))

        def _process_group(group: list[tuple[str, str, str, str, str]]) -> list[tuple[str, bool]]:
            results = []
            for src, ext, target_wav, target_dir, base_name in group:
                if self._cancel_requested:
                    break

                try:
                    if ext == ".fuz":
                        self._extract_fuz(src, target_wav, target_dir, base_name)
                    elif ext == ".xwm":
                        self._convert_xwm_to_wav(src, target_wav)
                    results.append((src, True))
                except Exception as e:
                    results.append((src, False))
                    _log.warning("Failed to process %s: %s", src, e)
            return results

        processed = 0
        failed = 0
        with ThreadPoolExecutor(max_workers=min(_audio_worker_count(), len(task_groups))) as pool:
            futures = [pool.submit(_process_group, group) for group in task_groups.values()]
            for future in as_completed(futures):
                for src, succeeded in future.result():
                    if succeeded:
                        processed += 1
                    else:
                        failed += 1
                    self._on_progress(processed + failed, total, os.path.basename(src))

        self._result_msg = f"Processed {processed}, failed {failed} of {total} files."
        if failed:
            self._error_msg = f"{failed} file(s) failed to extract."

    def _extract_fuz(self, src: str, target_wav: str, target_dir: str, base_name: str) -> None:
        """Extract a .fuz file to .wav via intermediate .xwm."""
        # BmlFuzDecode produces .xwm and .lip next to the .fuz file
        with tempfile.TemporaryDirectory() as tmp:
            temp_fuz = os.path.join(tmp, os.path.basename(src))
            shutil.copy2(src, temp_fuz)

            # Try to find BmlFuzDecode
            fuz_decode = self._find_tool("BmlFuzDecode.exe")
            if not fuz_decode:
                raise RuntimeError("BmlFuzDecode.exe not found")

            subprocess.run([fuz_decode, temp_fuz], capture_output=True, check=True)

            temp_xwm = os.path.join(tmp, base_name + ".xwm")
            temp_lip = os.path.join(tmp, base_name + ".lip")

            if not os.path.exists(temp_xwm):
                raise RuntimeError(f"FUZ decode did not produce XWM for {base_name}")

            self._convert_xwm_to_wav(temp_xwm, target_wav)

            if self._keep_intermediate:
                shutil.copy2(temp_xwm, os.path.join(target_dir, base_name + ".xwm"))
                if os.path.exists(temp_lip):
                    shutil.copy2(temp_lip, os.path.join(target_dir, base_name + ".lip"))

    def _convert_xwm_to_wav(self, xwm_path: str, wav_path: str) -> None:
        """Convert .xwm to .wav using xWMAEncode."""
        xwma_encode = self._find_tool("xWMAEncode.exe")
        if not xwma_encode:
            raise RuntimeError("xWMAEncode.exe not found")
        subprocess.run([xwma_encode, xwm_path, wav_path], capture_output=True, check=True)

    @staticmethod
    def _find_tool(name: str) -> str | None:
        from ui.tools.base import BaseTool
        return BaseTool.find_resource_tool(name) or None
