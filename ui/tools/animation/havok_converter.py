"""Batch-convert Havok packfiles and Fallout 4 assets for PS4."""
from __future__ import annotations

import logging
import os
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

from creation_lib.core.game_profiles import GAME_PROFILES
from creation_lib.ui.widgets import pick_folder
from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets.forms import begin_form, draw_path_row, end_form

_log = logging.getLogger("tools.havok_converter")

PS4_WORKERS = 16
PS4_ASSET_EXTENSIONS = frozenset({".hkx", ".nif", ".bto"})

# Games that have havok_version_id set
_GAMES = [
    (profile.display_name, str(profile.havok_version_id))
    for profile in GAME_PROFILES.values()
    if profile.havok_version_id is not None
]


def ps4_output_dir(input_dir: str) -> str:
    return os.path.join(os.path.normpath(input_dir), "ps4") if input_dir else ""


def ps4_destination_dir(input_dir: str, replace_source: bool) -> str:
    if not input_dir:
        return ""
    return os.path.normpath(input_dir) if replace_source else ps4_output_dir(input_dir)


def count_source_hkx(input_dir: str, excluded_dir: str = "") -> int:
    return count_source_ps4_assets(input_dir, excluded_dir).get(".hkx", 0)


def count_source_ps4_assets(input_dir: str, excluded_dir: str = "") -> dict[str, int]:
    if not input_dir or not os.path.isdir(input_dir):
        return {extension: 0 for extension in PS4_ASSET_EXTENSIONS}
    excluded = os.path.normcase(os.path.abspath(excluded_dir)) if excluded_dir else ""
    counts = {extension: 0 for extension in PS4_ASSET_EXTENSIONS}
    for root, dirs, files in os.walk(input_dir):
        if excluded:
            dirs[:] = [
                directory
                for directory in dirs
                if os.path.normcase(os.path.abspath(os.path.join(root, directory))) != excluded
            ]
        for filename in files:
            extension = Path(filename).suffix.lower()
            if extension in counts:
                counts[extension] += 1
    return counts


def run_ps4_asset_batch(
    source_dir: str,
    output_dir: str,
    skip_existing: bool,
    *,
    max_workers: int = PS4_WORKERS,
    havok_converter: Callable[[bytes], bytes] | None = None,
    nif_converter: Callable[[str, str], object] | None = None,
) -> dict:
    source_root = Path(source_dir).resolve(strict=False)
    output_root = Path(output_dir).resolve(strict=False)
    replacing_sources = os.path.normcase(str(source_root)) == os.path.normcase(str(output_root))
    excluded_root = source_root / "ps4" if replacing_sources else output_root
    files = [
        path
        for path in source_root.rglob("*")
        if path.is_file()
        and path.suffix.lower() in PS4_ASSET_EXTENSIONS
        and not path.resolve(strict=False).is_relative_to(excluded_root)
    ]
    files.sort(key=lambda path: os.path.normcase(str(path)))

    if havok_converter is None:
        from creation_lib._native import havok_native

        havok_converter = havok_native.havok_convert_ps4_bytes
    if nif_converter is None:
        from creation_lib.nif import native_runtime

        def nif_converter(source: str, destination: str) -> object:
            return native_runtime.nif_process_raw(
                source,
                destination,
                "ps4-converter",
                {},
            )

    def convert_one(source: Path) -> tuple[str, str, str]:
        destination = output_root / source.relative_to(source_root)
        if not replacing_sources and skip_existing and destination.exists():
            return "skipped", str(source), ""
        destination.parent.mkdir(parents=True, exist_ok=True)
        try:
            if source.suffix.lower() == ".hkx":
                converted = bytes(havok_converter(source.read_bytes()))
                temporary = destination.with_name(f".{destination.name}.ps4.tmp")
                temporary.write_bytes(converted)
                os.replace(temporary, destination)
            else:
                nif_converter(str(source), str(destination))
            return "converted", str(source), ""
        except Exception as exc:  # noqa: BLE001 - one bad asset must not cancel the batch
            return "error", str(source), str(exc)

    converted = 0
    skipped = 0
    errors = []
    with ThreadPoolExecutor(
        max_workers=max_workers,
        thread_name_prefix="ps4-asset-converter",
    ) as executor:
        futures = [executor.submit(convert_one, source) for source in files]
        for future in as_completed(futures):
            status, path, error = future.result()
            if status == "converted":
                converted += 1
            elif status == "skipped":
                skipped += 1
            else:
                errors.append({"path": path, "error": error})
    errors.sort(key=lambda entry: os.path.normcase(entry["path"]))
    return {
        "converted": converted,
        "skipped": skipped,
        "errors": errors,
        "workers": max_workers,
        "files": len(files),
    }


class HavokConverterTool(BaseTool):
    name = "PS4 / HKX Converter"
    tool_id = "havok_converter"
    description = "Recursively convert FO4 HKX, NIF, and BTO assets to PS4, or change HKX versions"
    category = "Havok"

    def __init__(self):
        super().__init__()
        self._input_dir = ""
        self._output_dir = ""
        self._ps4_mode = True
        self._replace_source = False
        self._source_idx = 0
        self._target_idx = 0
        self._preserve_structure = True
        self._skip_existing = True
        self._verbose = False
        self._log_lines: list[str] = []
        # Cached file count
        self._cached_count_key: tuple[str, bool] | None = None
        self._hkx_count = 0
        self._nif_count = 0
        self._bto_count = 0

    def draw_content(self) -> None:
        _, self._ps4_mode = imgui.checkbox(
            "Fallout 4 PS4 (64-bit generic layout)", self._ps4_mode
        )
        if self._ps4_mode:
            if self._replace_source:
                imgui.text_wrapped(
                    "Converts every HKX, NIF, and BTO under the selected folder in place. "
                    "The original files will be replaced."
                )
            else:
                imgui.text_wrapped(
                    "Converts every HKX, NIF, and BTO under the selected folder and preserves its relative path. "
                    "Original files are not modified."
                )
        else:
            game_names = [g[0] for g in _GAMES]
            _, self._source_idx = imgui.combo("Source Game", self._source_idx, game_names)
            _, self._target_idx = imgui.combo("Target Game", self._target_idx, game_names)

        imgui.separator()

        if begin_form("##havok_converter"):
            _, clicked = draw_path_row("Input", self._input_dir)
            if clicked:
                path = pick_folder("Select folder with HKX, NIF, and BTO files")
                if path:
                    self._input_dir = path

            if not self._ps4_mode:
                _, clicked = draw_path_row("Output", self._output_dir)
                if clicked:
                    path = pick_folder("Select output folder")
                    if path:
                        self._output_dir = path
            end_form()

        if self._ps4_mode:
            destination = ps4_destination_dir(self._input_dir, self._replace_source)
            label = "Replacing files in" if self._replace_source else "Output"
            imgui.text_wrapped(f"{label}: {destination or '[select an input folder]'}")

        imgui.separator()

        # Options
        if self._ps4_mode:
            _, self._replace_source = imgui.checkbox(
                "Replace source files", self._replace_source
            )
            if self._replace_source:
                imgui.text_wrapped(
                    "Warning: conversion will overwrite the source files without backups."
                )
            else:
                _, self._skip_existing = imgui.checkbox(
                    "Skip already converted", self._skip_existing
                )
        else:
            _, self._preserve_structure = imgui.checkbox(
                "Preserve directory structure", self._preserve_structure
            )
            _, self._skip_existing = imgui.checkbox(
                "Skip already converted", self._skip_existing
            )
        _, self._verbose = imgui.checkbox("Verbose logging", self._verbose)

        imgui.separator()

        # Count files (cached — recount only when input dir changes)
        count_key = (self._input_dir, self._ps4_mode)
        if self._cached_count_key != count_key:
            excluded = ps4_output_dir(self._input_dir) if self._ps4_mode else ""
            if self._ps4_mode:
                counts = count_source_ps4_assets(self._input_dir, excluded)
                self._hkx_count = counts[".hkx"]
                self._nif_count = counts[".nif"]
                self._bto_count = counts[".bto"]
            else:
                self._hkx_count = count_source_hkx(self._input_dir, excluded)
                self._nif_count = 0
                self._bto_count = 0
            self._cached_count_key = count_key

        # Convert button
        can_convert = (
            self._input_dir
            and (self._ps4_mode or self._output_dir)
            and (self._ps4_mode or self._source_idx != self._target_idx)
            and not self._running
        )
        if not can_convert:
            imgui.begin_disabled()
        if self._ps4_mode:
            button_label = (
                "Replace Source Assets with PS4"
                if self._replace_source
                else "Convert All to PS4"
            )
        else:
            button_label = "Convert All"
        if imgui.button(button_label):
            if self._ps4_mode and self._replace_source:
                imgui.open_popup("Replace source files?##havok_converter")
            else:
                self._start_conversion()
        if not can_convert:
            imgui.end_disabled()
        imgui.same_line()
        if self._ps4_mode:
            imgui.text_disabled(
                f"Found: {self._hkx_count} HKX, {self._nif_count} NIF, {self._bto_count} BTO (16 threads)"
            )
        else:
            imgui.text_disabled(f"Found: {self._hkx_count} .hkx files")

        self._draw_replace_confirmation()

        # Log
        if self._log_lines:
            imgui.separator()
            imgui.begin_child("log", imgui.ImVec2(0, 150), child_flags=imgui.ChildFlags_.borders)
            for line in self._log_lines[-100:]:
                imgui.text_wrapped(line)
            imgui.end_child()

    def _start_conversion(self):
        self._log_lines.clear()
        if self._ps4_mode:
            replace_source = self._replace_source
            destination = ps4_destination_dir(self._input_dir, replace_source)
            skip_existing = self._skip_existing and not replace_source
            self._start_batch(
                self._do_convert_ps4,
                destination,
                skip_existing,
            )
        else:
            target_ver = _GAMES[self._target_idx][1]
            self._start_batch(self._do_convert, target_ver)

    def _draw_replace_confirmation(self) -> None:
        opened, _ = imgui.begin_popup_modal(
            "Replace source files?##havok_converter",
            None,
            imgui.WindowFlags_.always_auto_resize,
        )
        if not opened:
            return

        imgui.text_wrapped(
            f"Replace {self._hkx_count + self._nif_count + self._bto_count} source asset(s) with their PS4 versions?"
        )
        imgui.text_wrapped("No backups will be created. This cannot be undone from the tool.")
        imgui.spacing()
        if imgui.button("Replace and Convert", imgui.ImVec2(150, 0)):
            imgui.close_current_popup()
            self._start_conversion()
        imgui.same_line()
        if imgui.button("Cancel", imgui.ImVec2(100, 0)):
            imgui.close_current_popup()
        imgui.end_popup()

    def _do_convert_ps4(
        self,
        output_dir: str,
        skip_existing: bool,
    ):
        source_dir = os.path.normpath(self._input_dir)
        result = run_ps4_asset_batch(
            source_dir,
            output_dir,
            skip_existing,
            max_workers=PS4_WORKERS,
        )
        self._set_result(result, output_dir)

    def _do_convert(self, target_version: str):
        import json

        from creation_lib._native import havok_native

        result_json = havok_native.havok_convert_batch(
            str(self._input_dir),
            str(self._output_dir),
            target_version,
            preserve_structure=self._preserve_structure,
        )
        result = json.loads(result_json)

        self._set_result(result, self._output_dir)

    def _set_result(self, result: dict, output_dir: str):
        converted = result.get("converted", 0)
        skipped = result.get("skipped", 0)
        errors = result.get("errors", [])

        self._result_msg = (
            f"Done: {converted} converted, {skipped} skipped, "
            f"{len(errors)} errors. Output: {output_dir}"
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
            "ps4_mode": True,
            "replace_source": False,
            "source_idx": 0,
            "target_idx": 0,
            "preserve_structure": True,
            "skip_existing": True,
            "verbose": False,
        }

    def apply_settings(self, settings: dict) -> None:
        self._ps4_mode = settings.get("ps4_mode", True)
        self._replace_source = settings.get("replace_source", False)
        self._source_idx = settings.get("source_idx", 0)
        self._target_idx = settings.get("target_idx", 0)
        self._preserve_structure = settings.get("preserve_structure", True)
        self._skip_existing = settings.get("skip_existing", True)
        self._verbose = settings.get("verbose", False)

    def collect_settings(self) -> dict:
        return {
            "ps4_mode": self._ps4_mode,
            "replace_source": self._replace_source,
            "source_idx": self._source_idx,
            "target_idx": self._target_idx,
            "preserve_structure": self._preserve_structure,
            "skip_existing": self._skip_existing,
            "verbose": self._verbose,
        }
