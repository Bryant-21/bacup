"""NIF to FBX Exporter tool — batch convert NIF meshes to FBX."""

from __future__ import annotations

import logging
import os

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row

_log = logging.getLogger("tools.nif_fbx_exporter")


class NifToFbxTool(BaseTool):
    name = "NIF to FBX"
    tool_id = "nif_fbx_exporter"
    description = "Batch convert NIF meshes to FBX"
    category = "NIF"

    def __init__(self):
        super().__init__()
        self._src_dir = ""
        self._out_dir = ""
        self._recurse = True
        self._include_skeleton = True
        self._include_weights = True
        self._include_materials = True

    def draw_content(self) -> None:
        if begin_form("##nif_fbx_exporter"):
            _, clicked = draw_path_row("Source", self._src_dir)
            if clicked:
                path = pick_folder("Select folder with NIF files")
                if path:
                    self._src_dir = path
                    if not self._out_dir:
                        base = os.path.basename(os.path.normpath(path))
                        self._out_dir = os.path.join(
                            os.path.dirname(path), base + "_fbx"
                        )

            _, clicked = draw_path_row("Output", self._out_dir)
            if clicked:
                path = pick_folder("Select output folder for FBX files")
                if path:
                    self._out_dir = path
            end_form()

        imgui.separator()
        _, self._recurse = imgui.checkbox("Include subdirectories", self._recurse)

        imgui.spacing()
        imgui.text("Export Options:")
        _, self._include_skeleton = imgui.checkbox(
            "Include skeleton", self._include_skeleton
        )
        _, self._include_weights = imgui.checkbox(
            "Include skin weights", self._include_weights
        )
        _, self._include_materials = imgui.checkbox(
            "Include materials", self._include_materials
        )

        imgui.spacing()
        imgui.separator()

        if not self._running:
            if imgui.button("Run", imgui.ImVec2(120, 0)):
                if not self._src_dir or not os.path.isdir(self._src_dir):
                    self._error_msg = "Please select a valid source folder."
                    return
                if not self._out_dir:
                    self._error_msg = "Please select an output folder."
                    return
                self._start_batch(self._run_export)
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

    def _run_export(self):
        try:
            from creation_lib.fbx import export_nif_to_fbx, FbxExportOptions
        except ImportError:
            self._error_msg = (
                "FBX export not available. "
                "Install Autodesk FBX SDK Python bindings."
            )
            return

        from creation_lib.nif import NifFile

        options = FbxExportOptions(
            include_skeleton=self._include_skeleton,
            include_weights=self._include_weights,
            include_materials=self._include_materials,
        )

        # Collect NIF files
        tasks = self.collect_files(
            self._src_dir,
            lambda p: p.lower().endswith(".nif"),
            include_subdirs=self._recurse,
        )

        if not tasks:
            self._error_msg = "No NIF files found in source folder."
            return

        os.makedirs(self._out_dir, exist_ok=True)

        exported = 0
        failed = 0

        for i, (nif_path, rel_dir) in enumerate(tasks):
            if self._cancel_requested:
                break

            filename = os.path.basename(nif_path)
            self._on_progress(i, len(tasks), f"Converting {filename}")

            try:
                nif = NifFile.load(nif_path)

                # Build output path preserving subdirectory structure
                if rel_dir:
                    out_subdir = os.path.join(self._out_dir, rel_dir)
                    os.makedirs(out_subdir, exist_ok=True)
                else:
                    out_subdir = self._out_dir

                out_name = os.path.splitext(filename)[0] + ".fbx"
                out_path = os.path.join(out_subdir, out_name)

                result = export_nif_to_fbx(nif, out_path, options)
                if result:
                    exported += 1
                else:
                    failed += 1
                    _log.warning("Failed to export: %s", nif_path)

            except Exception as e:
                failed += 1
                _log.warning("Error converting %s: %s", nif_path, e)

        self._on_progress(len(tasks), len(tasks), "Done")
        self._result_msg = (
            f"Converted {exported}/{len(tasks)} NIF files to FBX. "
            f"{failed} failed."
        )
