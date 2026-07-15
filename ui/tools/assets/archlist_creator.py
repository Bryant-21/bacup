"""Archlist Creator tool — generate .archlist files from directory contents."""

from __future__ import annotations

import logging
import os
from collections.abc import Callable
from pathlib import Path

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, pick_save_file

_log = logging.getLogger("tools.archlist_creator")

# Extensions to skip when scanning
_SKIP_EXTENSIONS = {".archlist", ".achlist", ".esp", ".ini"}


def _collect_archlist_entries(
    source_dir: Path,
    *,
    dest_prefix: str = "",
    skip_xml: bool = False,
    rel_mapper: Callable[[Path], str] | None = None,
) -> list[str]:
    """Collect Data-relative archlist entries from a source tree."""
    if not source_dir.is_dir():
        return []

    entries: list[str] = []
    for file_path in source_dir.rglob("*"):
        if not file_path.is_file():
            continue
        suffix = file_path.suffix.lower()
        if suffix in _SKIP_EXTENSIONS:
            continue
        if skip_xml and suffix == ".xml":
            continue
        rel_path = rel_mapper(file_path) if rel_mapper else file_path.relative_to(source_dir).as_posix()
        archive_path = Path("Data")
        if dest_prefix:
            archive_path = archive_path / dest_prefix
        archive_path = archive_path / rel_path
        entries.append(archive_path.as_posix())

    return sorted(set(entries))


def _write_archlist(output_file: str | Path, entries: list[str]) -> None:
    """Write a serialized archlist file."""
    output_path = Path(output_file)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    lines = ["["]
    for i, file_path in enumerate(entries):
        escaped = file_path.replace("\\", "\\\\").replace("/", "\\\\")
        comma = "," if i < len(entries) - 1 else ""
        lines.append(f'\t"{escaped}"{comma}')
    lines.append("]")

    output_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def create_archlist(directory_path: str, output_file: str) -> tuple[bool, int]:
    """Scan a directory recursively and create an archlist file.

    Returns (success, file_count).
    """
    source_dir = Path(directory_path)
    if not source_dir.is_dir():
        return False, 0

    all_files = _collect_archlist_entries(source_dir)
    _write_archlist(output_file, all_files)
    return True, len(all_files)


def create_loose_archlist(mod_dir: str, output_file: str) -> tuple[bool, int]:
    """Create an archlist for a mod's deployable loose-file roots.

    The file list mirrors ``deploy_loose_assets``:
    - ``data/`` is written under ``Data/``
    - top-level ``Meshes/`` is written under ``Data/Meshes/`` and skips source
      ``.xml`` files that are packed to ``.hkx`` in place
    - top-level ``Strings/`` is flattened to ``Data/Strings/<filename>``
    """
    mod_path = Path(mod_dir)
    if not mod_path.is_dir():
        return False, 0

    entries: list[str] = []
    entries.extend(_collect_archlist_entries(mod_path / "data"))
    entries.extend(
        _collect_archlist_entries(mod_path / "Meshes", dest_prefix="Meshes", skip_xml=True)
    )
    entries.extend(
        _collect_archlist_entries(
            mod_path / "Strings",
            dest_prefix="Strings",
            rel_mapper=lambda path: path.name,
        )
    )

    all_files = sorted(set(entries))
    _write_archlist(output_file, all_files)
    return True, len(all_files)


class ArchlistCreatorTool(BaseTool):
    name = "Archlist Creator"
    tool_id = "archlist_creator"
    description = "Generate .archlist from folder"
    category = "Mod Tools"

    def __init__(self):
        super().__init__()
        self._input_dir = ""
        self._output_file = ""

    def draw_content(self) -> None:
        if begin_form("##archlist_creator"):
            _, clicked = draw_path_row("Source", self._input_dir)
            if clicked:
                path = pick_folder("Select source directory")
                if path:
                    self._input_dir = path

            _, clicked = draw_path_row("Save As", self._output_file)
            if clicked:
                path = pick_save_file(
                    "Save archlist as",
                    [("Archlist", "*.achlist *.archlist"), ("All", "*.*")],
                    default_ext=".achlist",
                )
                if path:
                    self._output_file = path
            end_form()

        imgui.spacing()
        imgui.separator()
        imgui.spacing()

        if imgui.button("Create Archlist", imgui.ImVec2(160, 0)):
            if not self._input_dir:
                self._error_msg = "Please select a source directory."
                return
            if not os.path.isdir(self._input_dir):
                self._error_msg = f"Directory does not exist: {self._input_dir}"
                return
            if not self._output_file:
                self._error_msg = "Please select an output file path."
                return

            try:
                ok, count = create_archlist(self._input_dir, self._output_file)
                if ok:
                    self._result_msg = f"Archlist created: {count} files added.\n{self._output_file}"
                else:
                    self._error_msg = "No files found or operation failed."
            except Exception as e:
                self._error_msg = f"Failed to create archlist: {e}"
