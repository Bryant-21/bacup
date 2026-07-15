"""Cloth tree panel — hierarchical view of cloth data graph.

Left-side panel showing the cloth data structure as a collapsible
tree: SimClothData, particles, constraints, collidables, operators,
states.
"""
from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.cloth_maker.cloth_maker_app import ClothMakerApp

_log = logging.getLogger("cloth_maker.cloth_tree")


class ClothTreePanel:
    """Left-side panel: hierarchical cloth data tree."""

    def __init__(self, app: ClothMakerApp):
        self.app = app

    def draw(self) -> None:
        visible, _ = imgui.begin("Cloth Tree##cloth_maker")
        if not visible:
            imgui.end()
            return

        scene = self.app.scene

        if not scene.loaded:
            imgui.text_colored(
                imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                "No cloth data loaded.",
            )
            imgui.end()
            return

        cloth_json = scene.cloth_json
        if cloth_json is None:
            imgui.text("No cloth JSON available.")
            imgui.end()
            return

        leaf = imgui.TreeNodeFlags_.leaf.value
        default_open = imgui.TreeNodeFlags_.default_open.value

        cloth_name = cloth_json.get("name", "")
        if imgui.tree_node_ex(f'ClothData: "{cloth_name}"', default_open):
            # SimClothData entries
            for idx, scd in enumerate(cloth_json.get("sim_cloths", [])):
                scd_name = scd.get("name") or f"sim_cloth_{idx}"
                selected = (idx == scene.selected_sim_cloth_idx)
                flags = default_open
                if selected:
                    flags |= imgui.TreeNodeFlags_.selected.value

                if imgui.tree_node_ex(f'SimClothData[{idx}]: "{scd_name}"', flags):
                    if imgui.is_item_clicked():
                        scene.selected_sim_cloth_idx = idx
                        scene._extract_overlay_data()

                    self._draw_particles(scd)
                    self._draw_constraints(scd)
                    self._draw_collidables(scd)

                    imgui.tree_pop()

            imgui.tree_pop()

        imgui.end()

    def _draw_particles(self, scd: dict) -> None:
        particles = scd.get("particles", [])
        n = len(particles)
        fixed = len(scd.get("fixed_particle_indices", []))
        leaf = imgui.TreeNodeFlags_.leaf.value

        if imgui.tree_node_ex(f"Particles ({n})"):
            imgui.tree_node_ex(f"Fixed: {fixed}", leaf)
            imgui.tree_pop()
            imgui.tree_node_ex(f"Movable: {n - fixed}", leaf)
            imgui.tree_pop()
            imgui.tree_pop()

    def _draw_constraints(self, scd: dict) -> None:
        constraint_sets = scd.get("constraint_sets", [])
        total = len(constraint_sets)

        if imgui.tree_node_ex(f"Constraints ({total} sets)"):
            leaf = imgui.TreeNodeFlags_.leaf.value
            for cs in constraint_sets:
                cn = cs.get("class_name", "").replace("hcl", "").replace("ConstraintSet", "")
                count = cs.get("link_count", 0)
                imgui.tree_node_ex(f"{cn} ({count} links)", leaf)
                imgui.tree_pop()
            imgui.tree_pop()

    def _draw_collidables(self, scd: dict) -> None:
        collidables = scd.get("collidables", [])
        if imgui.tree_node_ex(f"Collidables ({len(collidables)})"):
            leaf = imgui.TreeNodeFlags_.leaf.value
            for col in collidables:
                name = col.get("name", "")
                shape_class = col.get("shape_class", "?")
                shape_type = shape_class.replace("hcl", "").replace("Shape", "")
                imgui.tree_node_ex(f"{shape_type}: {name}", leaf)
                imgui.tree_pop()
            imgui.tree_pop()
