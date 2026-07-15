"""Annotation Extractor tool — extract animation annotations and events from HKX XML files."""

from __future__ import annotations

import logging
import os
import xml.etree.ElementTree as ET

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, pick_file

_log = logging.getLogger("tools.annotation_extractor")


def _is_xml(path: str) -> bool:
    return os.path.splitext(path)[1].lower() == ".xml"


class AnnotationExtractorTool(BaseTool):
    name = "Annotation Extractor"
    tool_id = "annotation_extractor"
    description = "Extract annotations from HKX XMLs"
    category = "Havok"

    def __init__(self):
        super().__init__()
        self._input_path = ""
        self._output_dir = ""
        self._include_subdirs = True

    def draw_content(self) -> None:
        if begin_form("##annotation_extractor"):
            _, clicked = draw_path_row("Input", self._input_path)
            if clicked:
                path = pick_folder("Select folder with XML files")
                if not path:
                    path = pick_file("Select XML file", [("XML", "*.xml"), ("All", "*.*")])
                if path:
                    self._input_path = path

            _, clicked = draw_path_row("Output", self._output_dir)
            if clicked:
                path = pick_folder("Select output folder")
                if path:
                    self._output_dir = path
            end_form()

        imgui.separator()
        _, self._include_subdirs = imgui.checkbox("Include subdirectories", self._include_subdirs)

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
                self._start_batch(self._run_extract)
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

    def _collect_tasks(self) -> list[tuple[str, str]]:
        """Return list of (xml_path, rel_dir)."""
        return self.collect_files(self._input_path, _is_xml, self._include_subdirs)

    def _extract_from_file(self, path: str) -> tuple[set[str], set[str]]:
        """Extract annotations and event names from one XML file."""
        annotations: set[str] = set()
        event_names: set[str] = set()

        try:
            tree = ET.parse(path)
            root = tree.getroot()

            # Find hkaSplineCompressedAnimation annotations
            for anim in root.findall(".//hkobject[@class='hkaSplineCompressedAnimation']"):
                tracks = anim.find("./hkparam[@name='annotationTracks']")
                if tracks is not None:
                    for track_obj in tracks.findall("./hkobject"):
                        annots_param = track_obj.find("./hkparam[@name='annotations']")
                        if annots_param is not None:
                            for annot_obj in annots_param.findall("./hkobject"):
                                text_param = annot_obj.find("./hkparam[@name='text']")
                                if text_param is not None and text_param.text:
                                    annotations.add(text_param.text.strip())

            # Find hkbBehaviorGraphStringData event names
            for bsd in root.findall(".//hkobject[@class='hkbBehaviorGraphStringData']"):
                en_param = bsd.find("./hkparam[@name='eventNames']")
                if en_param is not None:
                    for cstring in en_param.findall("./hkcstring"):
                        if cstring.text:
                            event_names.add(cstring.text.strip())

        except Exception as e:
            _log.warning("Failed to parse %s: %s", path, e)

        return annotations, event_names

    def _run_extract(self):
        tasks = self._collect_tasks()
        total = len(tasks)
        if total == 0:
            self._result_msg = "No XML files found."
            return

        os.makedirs(self._output_dir, exist_ok=True)

        all_annotations: set[str] = set()
        all_event_names: set[str] = set()

        for i, (src, _rel) in enumerate(tasks):
            if self._cancel_requested:
                break

            self._on_progress(i, total, f"Processing: {os.path.basename(src)}")
            annotations, event_names = self._extract_from_file(src)
            all_annotations.update(annotations)
            all_event_names.update(event_names)

        annot_path = os.path.join(self._output_dir, "annotations.txt")
        with open(annot_path, "w", encoding="utf-8") as f:
            for annot in sorted(all_annotations):
                f.write(annot + "\n")

        events_path = os.path.join(self._output_dir, "eventNames.txt")
        with open(events_path, "w", encoding="utf-8") as f:
            for event in sorted(all_event_names):
                f.write(event + "\n")

        self._on_progress(total, total, "Done")
        self._result_msg = (
            f"Processed {total} files.\n"
            f"Annotations: {len(all_annotations)}\n"
            f"Event names: {len(all_event_names)}\n"
            f"Output: {self._output_dir}"
        )

    def get_default_settings(self) -> dict:
        return {"include_subdirs": True}

    def apply_settings(self, settings: dict) -> None:
        self._include_subdirs = settings.get("include_subdirs", True)

    def collect_settings(self) -> dict:
        return {"include_subdirs": self._include_subdirs}
