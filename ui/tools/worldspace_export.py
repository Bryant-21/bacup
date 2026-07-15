"""Worldspace Export tool."""

from __future__ import annotations

import os
from pathlib import Path

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import (
    begin_form,
    draw_combo_field,
    draw_path_row,
    end_form,
    pick_file,
    pick_save_file,
)


_GAMES = ["fo4", "fo76", "skyrimse", "starfield", "fo3", "fnv"]


class WorldspaceExportTool(BaseTool):
    name = "Worldspace Export"
    tool_id = "worldspace_export"
    description = "Export placed worldspace geometry from a plugin to a composed FBX scene."
    category = "NIF"

    def __init__(self):
        super().__init__()
        self._plugin_path = ""
        self._mesh_root = ""
        self._output_path = ""
        self._game_idx = 0
        self._bundle = None
        self._worldspaces = []
        self._worldspace_idx = 0
        self._cells = []
        self._selected_cells: set[int] = set()
        self._whole_worldspace = False
        self._normalize_origin = True

    def get_default_settings(self) -> dict:
        return {
            "plugin_path": "",
            "mesh_root": "",
            "output_path": "",
            "game": "fo4",
            "whole_worldspace": False,
            "normalize_origin": True,
        }

    def apply_settings(self, settings: dict) -> None:
        self._plugin_path = str(settings.get("plugin_path", self._plugin_path) or "")
        self._mesh_root = str(settings.get("mesh_root", self._mesh_root) or "")
        self._output_path = str(settings.get("output_path", self._output_path) or "")
        game = str(settings.get("game", "fo4") or "fo4")
        self._game_idx = _GAMES.index(game) if game in _GAMES else 0
        self._whole_worldspace = bool(settings.get("whole_worldspace", False))
        self._normalize_origin = bool(settings.get("normalize_origin", True))

    def collect_settings(self) -> dict:
        return {
            "plugin_path": self._plugin_path,
            "mesh_root": self._mesh_root,
            "output_path": self._output_path,
            "game": _GAMES[self._game_idx],
            "whole_worldspace": self._whole_worldspace,
            "normalize_origin": self._normalize_origin,
        }

    def draw_content(self) -> None:
        if begin_form("##worldspace_export_form"):
            _, clicked = draw_path_row("Plugin", self._plugin_path)
            if clicked:
                path = pick_file(
                    "Select plugin",
                    [("Bethesda plugins", "*.esp *.esm *.esl"), ("All files", "*.*")],
                )
                if path:
                    self._plugin_path = path
                    if not self._output_path:
                        self._output_path = str(Path(path).with_suffix(".fbx"))

            _, clicked = draw_path_row("Meshes", self._mesh_root)
            if clicked:
                path = pick_folder("Select extracted Meshes folder or Data folder")
                if path:
                    self._mesh_root = path

            _, clicked = draw_path_row("Output", self._output_path)
            if clicked:
                path = pick_save_file(
                    "Export FBX",
                    [("FBX", "*.fbx"), ("All files", "*.*")],
                    ".fbx",
                    initialfile=_default_output_name(self._plugin_path),
                )
                if path:
                    self._output_path = path

            _, self._game_idx = draw_combo_field("Game", _GAMES, self._game_idx)
            end_form()

        if imgui.button("Load Plugin##worldspace_export_load"):
            self._load_plugin()

        imgui.separator()
        self._draw_worldspace_controls()
        imgui.separator()
        _, self._normalize_origin = imgui.checkbox(
            "Normalize origin", self._normalize_origin
        )
        _, self._whole_worldspace = imgui.checkbox(
            "Export whole worldspace", self._whole_worldspace
        )
        self._draw_cell_list()

        can_export = bool(
            self._bundle
            and self._worldspaces
            and self._mesh_root
            and self._output_path
            and (self._whole_worldspace or self._selected_cells)
        )
        if self._running:
            imgui.begin_disabled()
        if not can_export:
            imgui.begin_disabled()
        if imgui.button("Export FBX##worldspace_export_run", imgui.ImVec2(140, 0)):
            self._start_batch(self._run_export)
        if not can_export:
            imgui.end_disabled()
        if self._running:
            imgui.end_disabled()

    def _draw_worldspace_controls(self) -> None:
        if not self._worldspaces:
            imgui.text_disabled("No plugin loaded.")
            return
        labels = [
            f"{w.editor_id or '<unnamed>'} ({w.form_id:06X})"
            for w in self._worldspaces
        ]
        self._worldspace_idx = max(0, min(self._worldspace_idx, len(labels) - 1))
        changed, self._worldspace_idx = imgui.combo(
            "Worldspace##worldspace_export_worldspace",
            self._worldspace_idx,
            labels,
        )
        if changed:
            self._refresh_cells()
        imgui.same_line()
        if imgui.button("Refresh Cells##worldspace_export_refresh_cells"):
            self._refresh_cells()

    def _draw_cell_list(self) -> None:
        if not self._cells or self._whole_worldspace:
            if self._cells:
                imgui.text_disabled(f"{len(self._cells)} cells selected by whole-world mode.")
            return
        imgui.text(f"Cells ({len(self._selected_cells)}/{len(self._cells)} selected)")
        if imgui.small_button("All##worldspace_export_cells_all"):
            self._selected_cells = {cell.form_id for cell in self._cells}
        imgui.same_line()
        if imgui.small_button("None##worldspace_export_cells_none"):
            self._selected_cells.clear()
        if imgui.begin_child("##worldspace_export_cells", imgui.ImVec2(0, 180), True):
            for cell in self._cells:
                selected = cell.form_id in self._selected_cells
                label = f"{cell.editor_id or '<unnamed>'} ({cell.form_id:06X})"
                changed, selected = imgui.checkbox(
                    f"{label}##worldspace_export_cell_{cell.form_id:08X}",
                    selected,
                )
                if changed:
                    if selected:
                        self._selected_cells.add(cell.form_id)
                    else:
                        self._selected_cells.discard(cell.form_id)
        imgui.end_child()

    def _load_plugin(self) -> None:
        if not self._plugin_path or not os.path.isfile(self._plugin_path):
            self._error_msg = "Select a valid plugin first."
            return
        try:
            from creation_lib.worldspace_export import load_plugin_bundle

            plugin_dir = Path(self._plugin_path).parent
            self._bundle = load_plugin_bundle(
                self._plugin_path,
                game=_GAMES[self._game_idx],
                master_search_paths=[plugin_dir],
            )
            self._worldspaces = self._bundle.list_worldspaces()
            self._worldspace_idx = 0
            self._refresh_cells()
            self._result_msg = f"Loaded {len(self._worldspaces)} worldspace(s)."
            self._error_msg = ""
        except Exception as exc:
            self._bundle = None
            self._worldspaces = []
            self._cells = []
            self._selected_cells.clear()
            self._error_msg = f"Plugin load failed: {exc}"

    def _refresh_cells(self) -> None:
        self._cells = []
        self._selected_cells.clear()
        if not self._bundle or not self._worldspaces:
            return
        worldspace = self._worldspaces[self._worldspace_idx]
        self._cells = self._bundle.list_cells(worldspace.form_id)
        if self._cells:
            self._selected_cells = {self._cells[0].form_id}

    def _run_export(self) -> None:
        if not self._bundle or not self._worldspaces:
            self._error_msg = "Load a plugin and select a worldspace first."
            return
        from creation_lib.worldspace_export import (
            build_export_manifest,
            export_manifest,
        )

        worldspace = self._worldspaces[self._worldspace_idx]
        selected_cells = None if self._whole_worldspace else set(self._selected_cells)
        self._on_progress(0, 4, "Extracting placements")
        placements = self._bundle.extract_placements(
            worldspace.form_id,
            selected_cells,
        )
        if self._cancel_requested:
            return
        self._on_progress(1, 4, "Resolving meshes")
        manifest = build_export_manifest(
            placements,
            [self._mesh_root],
            normalize_origin=self._normalize_origin,
        )
        if not manifest.placements:
            self._error_msg = (
                f"No exportable placements found. Skipped {len(manifest.skipped)}."
            )
            return
        if self._cancel_requested:
            return
        self._on_progress(2, 4, "Writing FBX")
        result = export_manifest(manifest, self._output_path)
        self._on_progress(4, 4, "Done")
        self._result_msg = (
            f"Exported {result.placement_count} placement(s) to {result.fbx_path}. "
            f"Skipped {result.skipped_count}; manifest: {result.manifest_path}."
        )


def _default_output_name(plugin_path: str) -> str:
    if not plugin_path:
        return "worldspace.fbx"
    return f"{Path(plugin_path).stem}_worldspace.fbx"
