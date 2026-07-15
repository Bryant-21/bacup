"""Folder Renamer tool — duplicate a folder with string replacements."""

from __future__ import annotations

import logging
import os
import shutil

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, draw_text_field

_log = logging.getLogger("tools.folder_renamer")

_TEXT_EXTENSIONS = (
    ".txt", ".py", ".js", ".html", ".css", ".json", ".xml", ".md",
    ".yml", ".yaml", ".ini", ".cfg", ".conf", ".java", ".cpp", ".c",
    ".h", ".hpp", ".php", ".rb", ".go", ".rs", ".ts", ".jsx", ".tsx",
    ".vue", ".svelte", ".sql", ".sh", ".bat", ".ps1", ".csv", ".toml",
    ".env", ".properties", ".gradle", ".kt", ".swift", ".dart",
)


def _replace_in_file(filepath: str, replacements: dict[str, str]) -> bool:
    """Replace strings in a text file. Returns True if changes were made."""
    try:
        with open(filepath, "r", encoding="utf-8") as f:
            content = f.read()
        new_content = content
        for old, new in replacements.items():
            new_content = new_content.replace(old, new)
        if new_content != content:
            with open(filepath, "w", encoding="utf-8") as f:
                f.write(new_content)
            return True
        return False
    except (UnicodeDecodeError, IOError):
        return False


def _parse_csv_replacements(csv_string: str, new_folder_name: str) -> dict[str, str]:
    """Parse CSV replacement string. Format: 'old1,old2' or 'old1:new1,old2:new2'."""
    replacements: dict[str, str] = {}
    if not csv_string:
        return replacements
    if ":" in csv_string:
        for item in csv_string.split(","):
            if ":" in item:
                old, new = item.split(":", 1)
                replacements[old.strip()] = new.strip()
            else:
                replacements[item.strip()] = new_folder_name
    else:
        for item in csv_string.split(","):
            replacements[item.strip()] = new_folder_name
    return replacements


class FolderRenamerTool(BaseTool):
    name = "Folder Renamer"
    tool_id = "folder_renamer"
    description = "Copy folder with string replacements"
    category = "Mod Tools"

    def __init__(self):
        super().__init__()
        self._source_folder = ""
        self._new_name = ""
        self._replacements_csv = ""

    def draw_content(self) -> None:
        if begin_form("##folder_renamer"):
            _, clicked = draw_path_row("Source", self._source_folder)
            if clicked:
                path = pick_folder("Select source folder")
                if path:
                    self._source_folder = path

            _, self._new_name = draw_text_field("New Name", self._new_name)
            _, self._replacements_csv = draw_text_field("Replace", self._replacements_csv)
            end_form()

        imgui.text_disabled("CSV: 'old1,old2' or 'old1:new1,old2:new2'")

        imgui.spacing()
        imgui.separator()
        imgui.spacing()

        if not self._running:
            if imgui.button("Copy & Rename", imgui.ImVec2(160, 0)):
                self._validate_and_run()
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

    def _validate_and_run(self):
        if not self._source_folder or not os.path.isdir(self._source_folder):
            self._error_msg = "Please select a valid source folder."
            return
        if not self._new_name.strip():
            self._error_msg = "Please enter a new folder name."
            return
        if not self._replacements_csv.strip():
            self._error_msg = "Please enter at least one string to replace."
            return

        dest_parent = os.path.dirname(self._source_folder)
        dest_path = os.path.join(dest_parent, self._new_name.strip())
        if os.path.exists(dest_path):
            self._error_msg = f"Destination already exists: {dest_path}"
            return

        self._start_batch(self._run_rename)

    def _run_rename(self):
        new_name = self._new_name.strip()
        replacements = _parse_csv_replacements(self._replacements_csv.strip(), new_name)

        source = self._source_folder
        dest_parent = os.path.dirname(source)
        dest_path = os.path.join(dest_parent, new_name)

        self._on_progress(0, 3, "Copying folder...")
        shutil.copytree(source, dest_path)

        if self._cancel_requested:
            return

        self._on_progress(1, 3, "Renaming files and directories...")

        # Walk bottom-up to rename dirs/files
        for root, dirs, files in os.walk(dest_path, topdown=False):
            if self._cancel_requested:
                return

            # Rename directories
            for dir_name in list(dirs):
                new_dir_name = dir_name
                for old, new in replacements.items():
                    new_dir_name = new_dir_name.replace(old, new)
                if new_dir_name != dir_name:
                    try:
                        os.rename(
                            os.path.join(root, dir_name),
                            os.path.join(root, new_dir_name),
                        )
                    except OSError as e:
                        _log.warning("Failed to rename dir %s: %s", dir_name, e)

            # Process files
            for file_name in files:
                new_file_name = file_name
                file_path = os.path.join(root, file_name)

                for old, new in replacements.items():
                    new_file_name = new_file_name.replace(old, new)

                if new_file_name != file_name:
                    new_file_path = os.path.join(root, new_file_name)
                    try:
                        os.rename(file_path, new_file_path)
                        file_path = new_file_path
                    except OSError as e:
                        _log.warning("Failed to rename file %s: %s", file_name, e)

                if file_path.lower().endswith(_TEXT_EXTENSIONS):
                    _replace_in_file(file_path, replacements)

        self._on_progress(3, 3, "Done")
        self._result_msg = f"Folder copied to: {dest_path}"
