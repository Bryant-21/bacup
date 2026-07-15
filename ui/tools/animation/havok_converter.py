"""Havok Version Converter — batch convert HKX files between game versions."""
from __future__ import annotations

import logging
import os

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row
from creation_lib.core.game_profiles import GAME_PROFILES

_log = logging.getLogger("tools.havok_converter")

# Games that have havok_version_id set
_GAMES = [(p.display_name, p.havok_version_id)
          for p in GAME_PROFILES.values()
          if p.havok_version_id is not None]


class HavokConverterTool(BaseTool):
    name = "HKX Converter"
    tool_id = "havok_converter"
    description = "Convert HKX files between Havok versions"
    category = "Havok"

    def __init__(self):
        super().__init__()
        self._input_dir = ""
        self._output_dir = ""
        self._source_idx = 0
        self._target_idx = 0
        self._preserve_structure = True
        self._skip_existing = True
        self._verbose = False
        self._log_lines: list[str] = []
        # Cached file count
        self._cached_count_dir = ""
        self._hkx_count = 0

    def draw_content(self) -> None:
        game_names = [g[0] for g in _GAMES]
        _, self._source_idx = imgui.combo("Source Game", self._source_idx, game_names)

        _, self._target_idx = imgui.combo("Target Game", self._target_idx, game_names)

        imgui.separator()

        if begin_form("##havok_converter"):
            _, clicked = draw_path_row("Input", self._input_dir)
            if clicked:
                path = pick_folder("Select folder with HKX files")
                if path:
                    self._input_dir = path

            _, clicked = draw_path_row("Output", self._output_dir)
            if clicked:
                path = pick_folder("Select output folder")
                if path:
                    self._output_dir = path
            end_form()

        imgui.separator()

        # Options
        _, self._preserve_structure = imgui.checkbox("Preserve directory structure", self._preserve_structure)
        _, self._skip_existing = imgui.checkbox("Skip already converted", self._skip_existing)
        _, self._verbose = imgui.checkbox("Verbose logging", self._verbose)

        imgui.separator()

        # Count files (cached — recount only when input dir changes)
        if self._cached_count_dir != self._input_dir:
            self._hkx_count = 0
            if self._input_dir and os.path.isdir(self._input_dir):
                for _root, _dirs, files in os.walk(self._input_dir):
                    self._hkx_count += sum(1 for f in files if f.lower().endswith('.hkx'))
            self._cached_count_dir = self._input_dir

        # Convert button
        can_convert = (
            self._input_dir and self._output_dir
            and self._source_idx != self._target_idx
            and not self._running
        )
        if not can_convert:
            imgui.begin_disabled()
        if imgui.button("Convert All"):
            self._start_conversion()
        if not can_convert:
            imgui.end_disabled()
        imgui.same_line()
        imgui.text_disabled(f"Found: {self._hkx_count} .hkx files")

        # Log
        if self._log_lines:
            imgui.separator()
            imgui.begin_child("log", imgui.ImVec2(0, 150), child_flags=imgui.ChildFlags_.borders)
            for line in self._log_lines[-100:]:
                imgui.text_wrapped(line)
            imgui.end_child()

    def _start_conversion(self):
        target_ver = _GAMES[self._target_idx][1]
        self._log_lines.clear()
        self._start_batch(self._do_convert, target_ver)

    def _do_convert(self, target_version: int):
        import json
        from creation_lib._native import havok_native

        result_json = havok_native.havok_convert_batch(
            str(self._input_dir),
            str(self._output_dir),
            target_version,
            preserve_structure=self._preserve_structure,
        )
        result = json.loads(result_json)

        converted = result.get("converted", 0)
        skipped = result.get("skipped", 0)
        errors = result.get("errors", [])

        self._result_msg = (
            f"Done: {converted} converted, {skipped} skipped, "
            f"{len(errors)} errors"
        )
        for entry in errors:
            if isinstance(entry, dict):
                path = entry.get("path", "?")
                err = entry.get("error", str(entry))
            else:
                path, err = (entry[0], entry[1]) if len(entry) == 2 else ("?", str(entry))
            self._log_lines.append(f"ERROR {path}: {err}")

    def get_default_settings(self) -> dict:
        return {
            "source_idx": 0,
            "target_idx": 0,
            "preserve_structure": True,
            "skip_existing": True,
            "verbose": False,
        }

    def apply_settings(self, settings: dict) -> None:
        self._source_idx = settings.get("source_idx", 0)
        self._target_idx = settings.get("target_idx", 0)
        self._preserve_structure = settings.get("preserve_structure", True)
        self._skip_existing = settings.get("skip_existing", True)
        self._verbose = settings.get("verbose", False)

    def collect_settings(self) -> dict:
        return {
            "source_idx": self._source_idx,
            "target_idx": self._target_idx,
            "preserve_structure": self._preserve_structure,
            "skip_existing": self._skip_existing,
            "verbose": self._verbose,
        }
