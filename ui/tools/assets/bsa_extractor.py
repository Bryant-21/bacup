"""BSA Extractor tool — pack/unpack Bethesda archives via native runtime."""

from __future__ import annotations

import logging
import os
from pathlib import Path
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed

from imgui_bundle import imgui

from creation_lib.ba2 import native_runtime
from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, draw_int_field, draw_combo_field, pick_file, pick_save_file

_log = logging.getLogger("tools.bsa_extractor")

GAME_FORMATS = [
    ("Fallout 4", "fo4", ".ba2"),
    ("Fallout 4 DDS", "fo4dds", ".ba2"),
    ("Fallout 4 Old Gen", "fo4og", ".ba2"),
    ("Fallout 4 Old Gen DDS", "fo4ogdds", ".ba2"),
    ("Fallout 76", "fo76", ".ba2"),
    ("Fallout 76 DDS", "fo76dds", ".ba2"),
    ("Skyrim SE/AE", "sse", ".bsa"),
    ("Skyrim LE", "tes5", ".bsa"),
    ("Fallout 3", "fo3", ".bsa"),
    ("Fallout: New Vegas", "fonv", ".bsa"),
    ("Starfield", "starfield", ".ba2"),
    ("Starfield DDS", "starfielddds", ".ba2"),
    ("Oblivion", "tes4", ".bsa"),
]

# Maps native archive type -> toolkit settings game key
_FORMAT_GAME_KEY = {
    "fo4": "fo4",
    "fo4dds": "fo4",
    "fo4og": "fo4",
    "fo4ogdds": "fo4",
    "fo76": "fo76",
    "fo76dds": "fo76",
    "sse": "skyrimse",
    "starfield": "starfield",
    "starfielddds": "starfield",
}


def _is_archive(path: str) -> bool:
    return os.path.splitext(path)[1].lower() in (".bsa", ".ba2")


class BSAExtractorTool(BaseTool):
    name = "BSA Extractor"
    tool_id = "bsa_extractor"
    description = "Extract/pack Bethesda archives"
    category = "Mod Tools"

    def __init__(self):
        super().__init__()
        self._input_path = ""
        self._output_path = ""
        self._pack_mode = False
        self._include_subdirs = False
        self._format_idx = 0
        self._compress = False
        self._workers = 4
        self._toolkit_settings = None

    def draw_content(self) -> None:
        if begin_form("##bsa_extractor"):
            _, clicked = draw_path_row("Input", self._input_path)
            if clicked:
                if self._pack_mode:
                    path = pick_folder("Select folder to pack")
                else:
                    path = pick_file("Select archive", [("Archives", "*.bsa *.ba2"), ("All", "*.*")])
                if path:
                    self._input_path = path

            _, clicked = draw_path_row("Output", self._output_path)
            if clicked:
                if self._pack_mode:
                    path = pick_save_file("Save archive as", [("Archives", "*.bsa *.ba2"), ("All", "*.*")])
                else:
                    path = pick_folder("Select output folder")
                if path:
                    self._output_path = path

            _, self._workers = draw_int_field("Workers", self._workers, min_val=1, max_val=16)
            _, self._format_idx = draw_combo_field("Format", [f[0] for f in GAME_FORMATS], self._format_idx)
            end_form()

        imgui.separator()

        _, self._pack_mode = imgui.checkbox("Pack mode", self._pack_mode)
        _, self._include_subdirs = imgui.checkbox("Include subdirectories", self._include_subdirs)
        _, self._compress = imgui.checkbox("Compress (-z)", self._compress)

        imgui.spacing()
        imgui.separator()

        # Run/cancel
        if not self._running:
            if imgui.button("Run", imgui.ImVec2(120, 0)):
                if not self._input_path:
                    self._error_msg = "Please select an input path."
                    return
                self._start_batch(self._run_archive)
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

    def _collect_tasks(self):
        """Return list of (file_path, rel_root) to process."""
        if self._pack_mode:
            return [(os.path.abspath(self._input_path), None)]
        return self.collect_files(self._input_path, _is_archive, self._include_subdirs)

    def _run_archive(self):
        """Worker: invoke native archive pack/unpack."""
        tasks = self._collect_tasks()
        total = len(tasks)
        if total == 0:
            self._result_msg = "No matching archives to process."
            return

        archive_type = GAME_FORMATS[self._format_idx][1]
        default_ext = GAME_FORMATS[self._format_idx][2]
        out_dir_base = self._output_path.strip()
        pack_mode = self._pack_mode
        compress = self._compress

        def _output_for(src: str, rel: str | None) -> str:
            if pack_mode:
                out = out_dir_base
                if not out:
                    out = src.rstrip("\\/") + default_ext
                return out
            if out_dir_base:
                if total > 1:
                    rel_dir = rel if (rel and rel != ".") else ""
                    bsa_name = os.path.splitext(os.path.basename(src))[0]
                    return os.path.join(out_dir_base, rel_dir, bsa_name) if rel_dir else os.path.join(out_dir_base, bsa_name)
                return out_dir_base
            return os.path.splitext(src)[0]

        def _run_one(src: str, rel: str | None) -> bool:
            try:
                out = _output_for(src, rel)
                if pack_mode:
                    _log.info("Native pack %s -> %s", archive_type, out)
                    native_runtime.pack_archive(
                        src,
                        out,
                        archive_type,
                        compress=compress,
                        compression_level=9,
                        share_data=False,
                    )
                else:
                    os.makedirs(out, exist_ok=True)
                    archive_format = "bsa" if src.lower().endswith(".bsa") else "ba2"
                    _log.info("Native extract %s -> %s", src, out)
                    native_runtime.extract_archive(src, out, format=archive_format, workers=1)
                return True
            except Exception:
                _log.exception("Native archive operation failed for %s", src)
                return False

        processed = 0
        failed = 0
        lock = threading.Lock()

        if pack_mode:
            batches = [[(src, rel, 1) for src, rel in tasks]]
        else:
            from creation_lib.preprocessor.extraction import plan_archive_extraction_batches

            rel_by_path = {os.path.abspath(src): rel for src, rel in tasks}
            batches = [
                [(str(task.archive), rel_by_path[os.path.abspath(str(task.archive))], task.file_workers) for task in batch]
                for batch in plan_archive_extraction_batches([Path(src) for src, _rel in tasks], self._workers)
            ]

        def _progress_for(src: str):
            def _progress(event: dict) -> bool:
                completed_files = int(event.get("completed", 0) or 0)
                total_archive_files = int(event.get("total", 0) or 0)
                self._status_msg = (
                    f"{os.path.basename(src)}: {completed_files:,}/{total_archive_files:,} file(s)"
                )
                return not self._cancel_requested

            return _progress

        def _run_task(src: str, rel: str | None, file_workers: int) -> bool:
            if pack_mode:
                return _run_one(src, rel)
            try:
                out = _output_for(src, rel)
                os.makedirs(out, exist_ok=True)
                archive_format = "bsa" if src.lower().endswith(".bsa") else "ba2"
                _log.info("Native extract %s -> %s", src, out)
                native_runtime.extract_archive(
                    src,
                    out,
                    format=archive_format,
                    workers=file_workers,
                    progress=_progress_for(src),
                )
                return True
            except Exception:
                _log.exception("Native archive operation failed for %s", src)
                return False

        for batch in batches:
            with ThreadPoolExecutor(max_workers=min(self._workers, len(batch))) as pool:
                future_map = {
                    pool.submit(_run_task, src, rel, file_workers): src
                    for src, rel, file_workers in batch
                    if not self._cancel_requested
                }
                for future in as_completed(future_map):
                    src = future_map[future]
                    ok = future.result()
                    with lock:
                        if ok:
                            processed += 1
                        else:
                            failed += 1
                        done = processed + failed
                    self._on_progress(done, total, f"Processing: {os.path.basename(src)}")

        self._on_progress(total, total, "Done")
        if failed:
            self._result_msg = f"Done: {processed} OK, {failed} failed"
        else:
            self._result_msg = f"Done: {processed} files processed"

        # Auto-save extracted_dir for known games after a successful unpack
        if not pack_mode and processed > 0 and out_dir_base and self._toolkit_settings:
            game_key = _FORMAT_GAME_KEY.get(archive_type)
            if game_key:
                self._toolkit_settings.set_game_extracted_dir(game_key, out_dir_base)
                _log.info("Set extracted_dir for %s → %s", game_key, out_dir_base)

    def get_default_settings(self) -> dict:
        return {
            "pack_mode": False,
            "include_subdirs": False,
            "format_idx": 0,
            "compress": False,
            "workers": 4,
        }

    def apply_settings(self, settings: dict) -> None:
        self._pack_mode = settings.get("pack_mode", False)
        self._include_subdirs = settings.get("include_subdirs", False)
        self._format_idx = settings.get("format_idx", 0)
        self._compress = settings.get("compress", False)
        self._workers = max(1, min(settings.get("workers", 4), 16))

    def collect_settings(self) -> dict:
        return {
            "pack_mode": self._pack_mode,
            "include_subdirs": self._include_subdirs,
            "format_idx": self._format_idx,
            "compress": self._compress,
            "workers": self._workers,
        }
