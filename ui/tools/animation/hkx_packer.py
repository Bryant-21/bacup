"""HKX Bulk Packer tool — pack/unpack HKX/XML files via pure Python hkxpack."""

from __future__ import annotations

import logging
import os
import shutil

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, draw_combo_field, pick_file

_log = logging.getLogger("tools.hkx_packer")


def _is_hkx(path: str) -> bool:
    return os.path.splitext(path)[1].lower() == ".hkx"


def _is_xml(path: str) -> bool:
    return os.path.splitext(path)[1].lower() == ".xml"


class HKXPackerTool(BaseTool):
    name = "HKX Packer"
    tool_id = "hkx_packer"
    description = "Bulk pack/unpack HKX files"
    category = "Havok"

    VERBOSITY_OPTIONS = ["Normal", "Quiet (-q)", "Verbose (-v)"]

    def __init__(self):
        super().__init__()
        self._input_path = ""
        self._output_dir = ""
        self._pack_mode = True  # True = pack XML->HKX, False = unpack HKX->XML
        self._include_subdirs = False
        self._verbosity_idx = 0  # 0=normal, 1=quiet, 2=verbose

    def draw_content(self) -> None:
        if begin_form("##hkx_packer"):
            _, clicked = draw_path_row("Input", self._input_path)
            if clicked:
                path = pick_folder("Select folder with HKX/XML files")
                if not path:
                    ext = "*.xml" if self._pack_mode else "*.hkx"
                    path = pick_file("Select file", [("HKX/XML", f"{ext}"), ("All", "*.*")])
                if path:
                    self._input_path = path

            _, clicked = draw_path_row("Output", self._output_dir)
            if clicked:
                path = pick_folder("Select output folder (optional)")
                if path:
                    self._output_dir = path

            _, self._verbosity_idx = draw_combo_field("Verbosity", self.VERBOSITY_OPTIONS, self._verbosity_idx)
            end_form()

        imgui.text_disabled("Leave empty to output next to source")
        imgui.separator()

        _, self._pack_mode = imgui.checkbox("Pack mode (XML -> HKX)", self._pack_mode)
        _, self._include_subdirs = imgui.checkbox("Include subdirectories", self._include_subdirs)

        imgui.spacing()
        imgui.separator()

        if not self._running:
            if imgui.button("Run", imgui.ImVec2(120, 0)):
                if not self._input_path:
                    self._error_msg = "Please select an input path."
                    return
                self._start_batch(self._run_hkxpack)
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

    def _collect_tasks(self) -> list[tuple[str, str | None]]:
        """Return list of (file_path, rel_dir) to process."""
        filt = _is_xml if self._pack_mode else _is_hkx
        return self.collect_files(self._input_path, filt, self._include_subdirs)

    def _run_hkxpack(self):
        import tempfile
        from pathlib import Path
        from creation_lib._native.havok_native import hkx_to_xml, xml_to_hkx

        tasks = self._collect_tasks()
        total = len(tasks)
        if total == 0:
            self._result_msg = "No matching files to process."
            return

        op = "pack" if self._pack_mode else "unpack"
        processed = 0
        failed = 0

        for i, (src, rel) in enumerate(tasks):
            if self._cancel_requested:
                break

            # Determine output dir
            out_dir = None
            if self._output_dir:
                rel_dir = rel if (rel and rel != ".") else ""
                out_dir = os.path.join(self._output_dir, rel_dir) if rel_dir else self._output_dir
                os.makedirs(out_dir, exist_ok=True)

            self._on_progress(i, total, f"{op}: {os.path.basename(src)}")

            try:
                if self._pack_mode:
                    out_path = os.path.splitext(src)[0] + ".hkx"
                    if out_dir:
                        out_path = os.path.join(out_dir, os.path.basename(out_path))
                    Path(out_path).write_bytes(xml_to_hkx(Path(src).read_text(encoding="utf-8")))
                else:
                    xml_str = hkx_to_xml(Path(src).read_bytes())
                    xml_name = os.path.splitext(os.path.basename(src))[0] + ".xml"
                    dest = os.path.join(out_dir, xml_name) if out_dir else os.path.join(os.path.dirname(src), xml_name)
                    Path(dest).write_text(xml_str, encoding="utf-8")
                processed += 1
            except Exception as e:
                _log.exception("HKXPack %s failed for %s", op, src)
                failed += 1

        self._on_progress(total, total, "Done")
        if failed:
            self._result_msg = f"Done: {processed} OK, {failed} failed"
        else:
            self._result_msg = f"Done: {processed} files processed"

    def get_default_settings(self) -> dict:
        return {"pack_mode": True, "include_subdirs": False, "verbosity_idx": 0}

    def apply_settings(self, settings: dict) -> None:
        self._pack_mode = settings.get("pack_mode", True)
        self._include_subdirs = settings.get("include_subdirs", False)
        self._verbosity_idx = settings.get("verbosity_idx", 0)

    def collect_settings(self) -> dict:
        return {
            "pack_mode": self._pack_mode,
            "include_subdirs": self._include_subdirs,
            "verbosity_idx": self._verbosity_idx,
        }
