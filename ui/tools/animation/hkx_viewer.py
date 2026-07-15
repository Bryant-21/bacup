"""HKX Viewer tool — inspect and save HKX/XML via XML text."""

from __future__ import annotations

import os
import shutil
import tempfile
from pathlib import Path

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from ui.tools.imgui_helpers import (
    begin_form,
    draw_path_row,
    end_form,
    pick_file,
    pick_save_file,
)


class HKXViewerTool(BaseTool):
    name = "HKX Viewer"
    tool_id = "hkx_viewer"
    description = "Unpack HKX to XML and inspect the converted text"
    category = "Havok"

    def __init__(self):
        super().__init__()
        self._input_path = ""
        self._xml_text = ""
        self._xml_path = ""
        self._dirty = False

    def draw_content(self) -> None:
        if begin_form("##hkx_viewer_form"):
            _, clicked = draw_path_row("HKX File", self._input_path)
            if clicked:
                path = self.open_file_dialog()
                if path:
                    self.open_file(path)
            end_form()

        imgui.spacing()
        if imgui.button("Refresh", imgui.ImVec2(120, 0)):
            if not self._input_path:
                self._error_msg = "Please select an HKX file."
            else:
                self.open_file(self._input_path)

        if self._input_path:
            imgui.same_line()
            label = os.path.basename(self._input_path)
            if self._dirty:
                label += " [Modified]"
            imgui.text_disabled(label)

        imgui.spacing()
        imgui.separator()
        imgui.spacing()

        if self._xml_text:
            if self._xml_path:
                imgui.text(f"Source: {self._input_path}")
                imgui.spacing()
            changed, text = imgui.input_text_multiline(
                "##hkx_xml_text",
                self._xml_text,
                imgui.ImVec2(-1, -1),
            )
            if changed:
                self._xml_text = text
                self._dirty = True
        else:
            imgui.text_disabled("Select an HKX file to inspect its unpacked XML.")

    def open_file(self, path: str) -> None:
        """Open an HKX or XML file into the XML viewer/editor."""
        normalized = os.path.normpath(path)
        ext = Path(normalized).suffix.lower()
        self._input_path = normalized
        self._xml_text = ""
        self._xml_path = ""
        self._error_msg = ""
        self._result_msg = ""
        self._dirty = False

        if ext == ".xml":
            try:
                self._xml_text = Path(normalized).read_text(encoding="utf-8", errors="replace")
                self._xml_path = normalized
                self._result_msg = f"Loaded XML: {os.path.basename(normalized)}"
            except Exception as e:
                self._error_msg = f"Failed to read XML: {e}"
            return

        if ext != ".hkx":
            self._error_msg = f"Unsupported file type: {ext or '[no extension]'}"
            return

        from creation_lib._native.havok_native import hkx_to_xml

        try:
            xml = hkx_to_xml(Path(normalized).read_bytes())
            fd, tmp_path = tempfile.mkstemp(suffix=".xml", prefix="hkx_viewer_")
            import io
            with io.open(fd, "w", encoding="utf-8") as f:
                f.write(xml)
            self._xml_text = xml
            self._xml_path = tmp_path
            self._result_msg = f"Loaded HKX: {os.path.basename(normalized)}"
        except Exception as e:
            self._error_msg = f"Failed to read unpacked XML: {e}"

    def open_file_dialog(self) -> str | None:
        """Open a native file dialog for HKX/XML files."""
        return pick_file("Select HKX/XML file", [("HKX/XML", "*.hkx *.xml"), ("All", "*.*")])

    def save_file_dialog(self) -> str | None:
        """Save current XML text as XML or repacked HKX."""
        return pick_save_file(
            "Save HKX/XML",
            [("HKX", "*.hkx"), ("XML", "*.xml"), ("All", "*.*")],
            default_ext="",
        )

    def save_file(self, path: str) -> None:
        """Save current XML text as XML or repacked HKX based on file extension."""
        normalized = os.path.normpath(path)
        ext = Path(normalized).suffix.lower()
        if not ext:
            normalized += ".xml"
            ext = ".xml"

        self._error_msg = ""
        self._result_msg = ""

        if ext == ".xml":
            try:
                Path(normalized).write_text(self._xml_text, encoding="utf-8")
                self._dirty = False
                self._result_msg = f"Saved XML: {os.path.basename(normalized)}"
            except Exception as e:
                self._error_msg = f"Failed to save XML: {e}"
            return

        if ext != ".hkx":
            self._error_msg = f"Unsupported save format: {ext}"
            return

        from creation_lib._native.havok_native import xml_to_hkx

        try:
            Path(normalized).write_bytes(xml_to_hkx(self._xml_text))
            self._dirty = False
            self._result_msg = f"Saved HKX: {os.path.basename(normalized)}"
        except Exception as e:
            self._error_msg = f"Failed to save HKX: {e}"
