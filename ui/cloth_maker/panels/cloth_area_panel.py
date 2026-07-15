"""Cloth Area panel — select and exclude mesh parts (trishapes) for cloth.

Owns two lists:

* Include — pick shapes to set/add as the cloth simulation area
* Exclude — pick shapes whose triangles the Region brush will not touch

Mutates ``app.region_panel._region_triangles`` via the include buttons.
The brush in Region panel reads ``_trishape_excluded`` for filtering.
"""
from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.cloth_maker.cloth_maker_app import ClothMakerApp

_log = logging.getLogger("cloth_maker.cloth_area_panel")


class ClothAreaPanel:
    """Trishape include/exclude selector for cloth region authoring."""

    def __init__(self, app: ClothMakerApp):
        self.app = app
        # block_index -> checked
        self._trishape_selected: dict[int, bool] = {}
        self._trishape_excluded: dict[int, bool] = {}

    # ------------------------------------------------------------------
    # Public query used by RegionPanel brush
    # ------------------------------------------------------------------

    def get_excluded_triangle_set(self) -> set[int]:
        """Return the set of merged-triangle indices covered by excluded shapes."""
        infos = self.app.trishape_infos
        if not infos:
            return set()
        out: set[int] = set()
        for info in infos:
            if self._trishape_excluded.get(info.block_index, False):
                out.update(range(info.tri_start, info.tri_end))
        return out

    def get_excluded_vertex_set(self) -> set[int]:
        """Return the set of vertex indices belonging to excluded shapes.

        Computed via triangle->vertex lookup on the merged skin_data, which
        is correct because each vertex in the merged array belongs to exactly
        one trishape.
        """
        excluded_tris = self.get_excluded_triangle_set()
        if not excluded_tris:
            return set()
        sd = self.app.skin_data
        if sd is None or sd.triangles is None:
            return set()
        tri_arr = sd.triangles[sorted(excluded_tris)]
        return set(int(v) for v in tri_arr.ravel())

    # ------------------------------------------------------------------
    # Drawing
    # ------------------------------------------------------------------

    def draw(self) -> None:
        visible, _ = imgui.begin("Cloth Area##cloth_maker")
        if not visible:
            imgui.end()
            return

        infos = self.app.trishape_infos
        if not infos:
            imgui.text_colored(
                imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                "Import a NIF mesh first.",
            )
            imgui.end()
            return

        # Keep dicts in sync with loaded shapes
        for info in infos:
            self._trishape_selected.setdefault(info.block_index, False)
            self._trishape_excluded.setdefault(info.block_index, False)

        imgui.text_disabled(
            "Pick shapes to include in the cloth area, and shapes to\n"
            "exclude from brush painting."
        )
        imgui.spacing()

        self._draw_include_section(infos)
        imgui.spacing()
        imgui.separator()
        imgui.spacing()
        self._draw_exclude_section(infos)

        imgui.end()

    # ------------------------------------------------------------------
    # Include list — mutates region_panel._region_triangles
    # ------------------------------------------------------------------

    def _draw_include_section(self, infos: list) -> None:
        imgui.separator_text("Include in Cloth Area")
        imgui.text_disabled(
            "Check shapes, then click 'Use as Cloth Area' or 'Add to Cloth Area'."
        )

        avail = imgui.get_content_region_avail()
        list_height = max(120.0, avail.y * 0.45)
        if imgui.begin_child(
            "##trishape_include_list", imgui.ImVec2(0, list_height),
            child_flags=imgui.ChildFlags_.borders.value,
        ):
            for info in infos:
                label = (
                    f"{info.name}  "
                    f"({info.num_vertices}v, {info.num_triangles}t)"
                    f"##inc_{info.block_index}"
                )
                changed, checked = imgui.checkbox(
                    label, self._trishape_selected[info.block_index],
                )
                if changed:
                    self._trishape_selected[info.block_index] = checked
        imgui.end_child()

        any_checked = any(
            self._trishape_selected.get(i.block_index, False) for i in infos
        )

        if not any_checked:
            imgui.begin_disabled()
        if imgui.button("Use as Cloth Area", imgui.ImVec2(150, 0)):
            self._apply_include(replace=True)
        imgui.set_item_tooltip(
            "Replace current cloth area with the checked shapes"
        )
        imgui.same_line()
        if imgui.button("Add to Cloth Area", imgui.ImVec2(150, 0)):
            self._apply_include(replace=False)
        imgui.set_item_tooltip(
            "Add the checked shapes to the existing cloth area"
        )
        if not any_checked:
            imgui.end_disabled()

        imgui.same_line()
        if imgui.button("All##inc"):
            for info in infos:
                self._trishape_selected[info.block_index] = True
        imgui.same_line()
        if imgui.button("None##inc"):
            for info in infos:
                self._trishape_selected[info.block_index] = False

    def _apply_include(self, replace: bool) -> None:
        infos = self.app.trishape_infos
        rp = self.app.region_panel
        if not infos or rp is None:
            return

        new_tris: set[int] = set()
        for info in infos:
            if self._trishape_selected.get(info.block_index, False):
                new_tris.update(range(info.tri_start, info.tri_end))
        if not new_tris:
            return

        if replace:
            rp._region_triangles = new_tris
        else:
            rp._region_triangles |= new_tris

        rp._generate_error = ""
        rp._generate_success = ""

        names = [
            i.name for i in infos
            if self._trishape_selected.get(i.block_index, False)
        ]
        _log.info(
            "Cloth area from shapes [%s]: %d triangles (replace=%s)",
            ", ".join(names), len(new_tris), replace,
        )

    # ------------------------------------------------------------------
    # Exclude list — consumed by region_panel brush
    # ------------------------------------------------------------------

    def _draw_exclude_section(self, infos: list) -> None:
        imgui.separator_text("Exclude from Painting")
        imgui.text_disabled(
            "Checked shapes will NOT be affected by the Region or Pin brush."
        )

        avail = imgui.get_content_region_avail()
        list_height = max(120.0, avail.y - 40.0)
        if imgui.begin_child(
            "##trishape_exclude_list", imgui.ImVec2(0, list_height),
            child_flags=imgui.ChildFlags_.borders.value,
        ):
            for info in infos:
                label = (
                    f"{info.name}  "
                    f"({info.num_vertices}v, {info.num_triangles}t)"
                    f"##exc_{info.block_index}"
                )
                changed, checked = imgui.checkbox(
                    label, self._trishape_excluded[info.block_index],
                )
                if changed:
                    self._trishape_excluded[info.block_index] = checked
        imgui.end_child()

        if imgui.button("All##exc"):
            for info in infos:
                self._trishape_excluded[info.block_index] = True
        imgui.same_line()
        if imgui.button("None##exc"):
            for info in infos:
                self._trishape_excluded[info.block_index] = False
