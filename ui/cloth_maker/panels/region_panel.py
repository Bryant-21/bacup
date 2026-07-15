"""Region panel — parametric cloth region painting and generation.

Paint cloth region triangles and pin vertices on the mesh, pick
topology preset and material, then generate full cloth setup
(constraints, bones, capsules, skin weights).
"""
from __future__ import annotations

import logging
from typing import TYPE_CHECKING

import numpy as np
from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.cloth_maker.cloth_maker_app import ClothMakerApp

_log = logging.getLogger("cloth_maker.region_panel")


def _build_bend_quads(
    triangles: list[tuple[int, int, int]],
    topology_name: str,
) -> list[tuple[int, int, int, int, float]]:
    """Compute (a,b,c,d,stiffness) bend quads from a triangle list.

    For every undirected edge shared by exactly two triangles, emit a quad
    where ``a,b`` are the shared edge endpoints and ``c,d`` are the tip
    vertices of the two adjacent triangles (the vertex of each triangle
    that is not on the shared edge). The solver uses these to build
    dihedral angle constraints, which is what keeps a flat sheet from
    folding along every interior edge.
    """
    try:
        import json as _json
        from creation_lib._native import havok_native as _hn
        preset = _json.loads(_hn.cloth_topology_get(topology_name))
    except Exception:
        return []

    if not preset.get("use_bend_stiffness", False):
        return []
    stiff = float(preset.get("bend_stiffness", 0.0))
    if stiff <= 0.0:
        return []

    from collections import defaultdict
    edge_map: dict[tuple[int, int], list[int]] = defaultdict(list)
    for pa, pb, pc in triangles:
        for e_a, e_b, opp in ((pa, pb, pc), (pb, pc, pa), (pc, pa, pb)):
            key = (e_a, e_b) if e_a < e_b else (e_b, e_a)
            edge_map[key].append(int(opp))

    out: list[tuple[int, int, int, int, float]] = []
    for (a, b), opps in edge_map.items():
        if len(opps) == 2:
            out.append((int(a), int(b), opps[0], opps[1], stiff))
    return out

# Topology presets — controls constraint generation strategy
# Keys must match py_creation_lib/python/creation_lib/havok_cloth/topology.py TOPOLOGY_PRESETS names
_TOPOLOGY_PRESETS = [
    ("thin_cloth", "Thin fabric (single layer, light constraints)"),
    ("thick_cloth", "Thick fabric (double-sided, stiffer constraints)"),
    ("chain", "Chain mail (high mass, low bend stiffness)"),
    ("skirt_flaps", "Skirt/flaps (vertical strips, strong stretch)"),
    ("soft_body", "Soft body (volume-preserving, all-around constraints)"),
]

# Material presets (shared with param_panel)
_MATERIAL_PRESETS = [
    "Silk", "Cotton", "Linen", "Denim",
    "Leather", "Heavy Wool", "Chain", "Rope",
]


class RegionPanel:
    """Region painting and cloth generation panel."""

    def __init__(self, app: ClothMakerApp):
        self.app = app

        # Brush mode
        self._brush_active: bool = False  # Must be toggled on to paint
        self._brush_mode: str = "region"  # "region" or "pin"
        self._brush_radius: float = 5.0
        self._brush_strength: float = 1.0

        # Painted data (triangle and vertex masks)
        self._region_triangles: set[int] = set()
        self._pin_vertices: set[int] = set()

        # Preset selection
        self._topology_idx: int = 0
        self._material_idx: int = 1  # Cotton default

        # Generation state
        self._generating: bool = False
        self._generate_error: str = ""
        self._generate_success: str = ""
        self._generate_progress: float = 0.0
        self._generate_status: str = ""

    @property
    def has_mesh(self) -> bool:
        return self.app.nif_file is not None

    @property
    def has_region(self) -> bool:
        return len(self._region_triangles) > 0

    @property
    def has_pins(self) -> bool:
        return len(self._pin_vertices) > 0

    def draw(self) -> None:
        visible, _ = imgui.begin("Region##cloth_maker")
        if not visible:
            imgui.end()
            return

        if not self.has_mesh:
            imgui.text_colored(
                imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                "Import a NIF mesh first.",
            )
            imgui.spacing()
            imgui.text_disabled(
                "Use File > Import NIF to load a mesh,\n"
                "then paint cloth regions here."
            )
            imgui.end()
            return

        imgui.text_disabled(
            "Use the Cloth Area panel to pick or exclude mesh parts.",
        )
        imgui.spacing()
        self._draw_cloth_target_picker()
        imgui.spacing()
        self._draw_brush_controls()
        imgui.spacing()
        self._draw_region_info()
        imgui.spacing()
        self._draw_topology_picker()
        imgui.spacing()
        self._draw_material_picker()
        imgui.spacing()
        self._draw_generate_section()

        imgui.end()

    def _draw_cloth_target_picker(self) -> None:
        """Combo selecting which BSTriShape becomes the cloth on export.

        Export calls ``cloth_skin_bind`` on this one shape only. Defaults
        to the first BSTriShape on import; the user can switch targets
        here when the NIF has multiple shapes (e.g. bathrobe + hoodie).
        """
        imgui.separator_text("Cloth Target Shape")

        infos = self.app.trishape_infos
        if not infos:
            imgui.text_disabled("No BSTriShape in NIF")
            return

        labels = [f"{info.name} ({info.num_vertices}v)" for info in infos]
        current = 0
        for i, info in enumerate(infos):
            if info.block_index == self.app.cloth_target_shape_id:
                current = i
                break

        changed, new_idx = imgui.combo(
            "Shape##cloth_target", current, labels,
        )
        if changed:
            self.app.cloth_target_shape_id = infos[new_idx].block_index

        imgui.text_disabled(
            "On Export, this shape is skinned to cluster bones\n"
            "and promoted to BSSubIndexTriShape."
        )

    def _draw_brush_controls(self) -> None:
        imgui.separator_text("Brush")

        changed, self._brush_active = imgui.checkbox(
            "Enable Brush##region_brush", self._brush_active,
        )
        imgui.set_item_tooltip("Toggle viewport brush painting (LMB to paint)")
        if changed and self._brush_active:
            # Brushes are mutually exclusive — disable the authoring brush.
            ap = getattr(self.app, "authoring_panel", None)
            if ap is not None and ap._brush_active:
                ap._brush_active = False

        if not self._brush_active:
            imgui.text_disabled("Enable brush to paint on the mesh.")
            return

        # Mode toggle buttons
        region_active = self._brush_mode == "region"
        pin_active = self._brush_mode == "pin"

        if region_active:
            imgui.push_style_color(
                imgui.Col_.button, imgui.ImVec4(0.2, 0.5, 0.7, 1.0))
        if imgui.button("Region Brush", imgui.ImVec2(120, 0)):
            self._brush_mode = "region"
        if region_active:
            imgui.pop_style_color()
        imgui.set_item_tooltip("Paint triangles as cloth region (LMB to add, Shift+LMB to erase)")

        imgui.same_line()

        if pin_active:
            imgui.push_style_color(
                imgui.Col_.button, imgui.ImVec4(0.7, 0.5, 0.2, 1.0))
        if imgui.button("Pin Brush", imgui.ImVec2(120, 0)):
            self._brush_mode = "pin"
        if pin_active:
            imgui.pop_style_color()
        imgui.set_item_tooltip("Paint vertices as fixed/pinned (attach points)")

        imgui.text_disabled("Shift+LMB to erase")

        # Brush parameters
        _, self._brush_radius = imgui.slider_float(
            "Radius##region_brush", self._brush_radius, 1.0, 50.0, "%.1f",
        )

        _, self._brush_strength = imgui.slider_float(
            "Strength##region_brush", self._brush_strength, 0.1, 1.0, "%.2f",
        )

        # Quick actions
        if imgui.button("Clear Cloth Area"):
            self._region_triangles.clear()
            self._generate_error = ""
            self._generate_success = ""
        imgui.same_line()
        if imgui.button("Clear Pins"):
            self._pin_vertices.clear()
        imgui.same_line()
        if imgui.button("Select All"):
            self._select_all_triangles()

    def _draw_region_info(self) -> None:
        imgui.separator_text("Cloth Area Status")

        tri_count = len(self._region_triangles)
        pin_count = len(self._pin_vertices)

        if tri_count > 0:
            imgui.text(f"Cloth triangles: {tri_count}")
        else:
            imgui.text_colored(
                imgui.ImVec4(0.8, 0.6, 0.2, 1.0),
                "No cloth area set — select shapes above or paint with brush",
            )

        if pin_count > 0:
            imgui.text(f"Pin vertices: {pin_count}")
        else:
            imgui.text_disabled("No pins painted (optional — auto-detected from top edge)")

        # Estimate particle count from region
        if tri_count > 0:
            # Rough estimate: ~vertices = triangles * 0.5 (shared vertices)
            est_particles = max(3, int(tri_count * 0.5))
            imgui.text_disabled(f"Estimated particles: ~{est_particles}")

    def _draw_topology_picker(self) -> None:
        imgui.separator_text("Topology")

        names = [t[0] for t in _TOPOLOGY_PRESETS]
        changed, self._topology_idx = imgui.combo(
            "Preset##topology", self._topology_idx, names,
        )

        # Show description
        if 0 <= self._topology_idx < len(_TOPOLOGY_PRESETS):
            desc = _TOPOLOGY_PRESETS[self._topology_idx][1]
            imgui.text_disabled(desc)

    def _draw_material_picker(self) -> None:
        imgui.separator_text("Material")

        changed, self._material_idx = imgui.combo(
            "Preset##material", self._material_idx, _MATERIAL_PRESETS,
        )

    def _draw_generate_section(self) -> None:
        imgui.separator()
        imgui.spacing()

        can_generate = self.has_region and not self._generating

        if not can_generate:
            imgui.begin_disabled()
        if imgui.button("Generate Cloth", imgui.ImVec2(-1, 30)):
            self._generate_cloth()
        if not can_generate:
            imgui.end_disabled()

        if self._generating:
            imgui.spacing()
            imgui.progress_bar(self._generate_progress, imgui.ImVec2(-1, 0))
            if self._generate_status:
                imgui.text(self._generate_status)

        if self._generate_error:
            imgui.spacing()
            imgui.text_colored(
                imgui.ImVec4(1.0, 0.3, 0.3, 1.0),
                f"Error: {self._generate_error}",
            )

        if self._generate_success:
            imgui.spacing()
            imgui.text_colored(
                imgui.ImVec4(0.3, 1.0, 0.3, 1.0),
                self._generate_success,
            )

        imgui.spacing()
        imgui.text_disabled("Generates constraints, bones, capsules,")
        imgui.text_disabled("and skin weights from the painted region.")

    def _select_all_triangles(self) -> None:
        """Select all triangles in the mesh as the cloth region."""
        if not self.has_mesh:
            return

        try:
            from creation_lib.skinning.reference_body import extract_skin_data_from_nif
            skin_data = extract_skin_data_from_nif(self.app.scene.nif_path)
            if skin_data and skin_data.triangles is not None:
                self._region_triangles = set(range(len(skin_data.triangles)))
                _log.info("Selected all %d triangles", len(self._region_triangles))
        except Exception as e:
            _log.warning("Failed to select all triangles: %s", e)

    def _generate_cloth(self) -> None:
        """Generate cloth setup from painted region."""
        if not self.has_region:
            return

        topology_name = _TOPOLOGY_PRESETS[self._topology_idx][0]
        material_name = _MATERIAL_PRESETS[self._material_idx]

        self._generating = True
        self._generate_error = ""
        self._generate_success = ""
        self._generate_progress = 0.0
        self._generate_status = "Initializing..."

        try:
            # Need skin_data to get vertex positions and triangles
            sd = self.app.skin_data
            if sd is None:
                self._generate_error = "No skin data available — import a NIF first"
                return

            self._generate_progress = 0.1
            self._generate_status = "Extracting region mesh..."

            # Extract region vertices and triangles from skin_data
            region_tri_indices = sorted(self._region_triangles)
            region_tris = sd.triangles[region_tri_indices]  # (R, 3)

            # Remap: collect unique vertex indices, build new triangle array
            unique_verts = np.unique(region_tris.ravel())
            old_to_new = {int(old): new for new, old in enumerate(unique_verts)}
            positions = [(float(sd.vertices[v][0]), float(sd.vertices[v][1]),
                          float(sd.vertices[v][2]), 0.0) for v in unique_verts]
            triangles = [(old_to_new[int(region_tris[i][0])],
                          old_to_new[int(region_tris[i][1])],
                          old_to_new[int(region_tris[i][2])])
                         for i in range(len(region_tris))]

            # Remap pin vertices to new indices
            fixed_indices = [old_to_new[v] for v in sorted(self._pin_vertices)
                            if v in old_to_new]

            self._generate_progress = 0.3
            self._generate_status = "Generating cloth setup..."

            import json as _json
            from creation_lib._native import havok_native as _hn

            region_input = {
                "name": "ClothRegion",
                "positions": positions,
                "triangles": triangles,
                "fixed_indices": fixed_indices,
                "topology_name": topology_name,
            }
            if material_name:
                region_input["material_name"] = material_name
            result = _json.loads(_hn.cloth_region_generate(_json.dumps(region_input)))

            self._generate_progress = 0.7
            self._generate_status = "Baking cloth data..."

            blob = bytes(_hn.cloth_bake(_json.dumps(result["setup"])))

            self._generate_progress = 0.9
            self._generate_status = "Updating scene..."

            # Generated positions are already in NIF space — no Havok offset
            self.app.scene._z_offset = 0.0
            self.app.scene.refresh_from_blob(blob)

            # Build 4-particle bend quads from the triangle list. The HKX
            # baker only stores particleA/particleB for bend links, so the
            # HKX-based extractor in cloth_scene never finds C/D. We have
            # the triangles right here in particle-index space, so we can
            # recover the dihedral structure directly.
            self.app.scene.bend_quads = _build_bend_quads(
                triangles, topology_name,
            )
            _log.info(
                "Generated %d bend quads from %d triangles",
                len(self.app.scene.bend_quads), len(triangles),
            )

            # Store particle↔vertex mappings for mesh deformation preview.
            p2v = unique_verts.copy().astype(np.intp)
            self.app.scene.particle_to_vertex = p2v
            # Build reverse mapping: vertex→particle
            n_verts = len(sd.vertices)
            v2p = np.full(n_verts, -1, dtype=np.intp)
            for pi, vi in enumerate(p2v):
                if vi < n_verts:
                    v2p[vi] = pi
            self.app.scene.vertex_to_particle = v2p

            # Invalidate dependent panels
            if self.app.param_panel:
                self.app.param_panel._dirty = True
            if self.app.preview_panel and self.app.preview_panel.solver:
                self.app.preview_panel.solver = None

            particle_count = len(positions)
            constraint_count = len(triangles)
            self._generate_progress = 1.0
            self._generate_status = ""
            self._generate_success = (
                f"Generated: {particle_count} particles, "
                f"{constraint_count} triangles "
                f"({topology_name}, {material_name})"
            )
            self.app.status_text = self._generate_success
            _log.info("Generated cloth: topology=%s, material=%s",
                      topology_name, material_name)

        except ImportError as e:
            self._generate_error = f"Generation module not available: {e}"
            _log.warning("Cloth generation not available: %s", e)
        except Exception as e:
            self._generate_error = str(e)
            _log.error("Cloth generation failed: %s", e, exc_info=True)
        finally:
            self._generating = False
            self._generate_progress = 0.0
            self._generate_status = ""

    def handle_brush_input(self, hit_tri_idx: int, hit_point: np.ndarray,
                           vertices: np.ndarray, triangles: np.ndarray,
                           erasing: bool = False) -> None:
        """Handle a brush stroke hit on the mesh.

        Called from the viewport when the user clicks/drags on the mesh
        while in region painting mode.

        Args:
            hit_tri_idx: Triangle index under the cursor
            hit_point: World-space hit position
            vertices: (V, 3) mesh vertices
            triangles: (T, 3) triangle indices
            erasing: True if shift is held (erase mode)
        """
        if hit_tri_idx < 0:
            return

        if self._brush_mode == "region":
            # Find triangles within brush radius
            affected = self._get_triangles_in_radius(
                hit_point, vertices, triangles,
            )
            if erasing:
                self._region_triangles -= affected
            else:
                self._region_triangles |= affected

        elif self._brush_mode == "pin":
            pd = self.app.scene.particle_data
            if self.app.scene.loaded and pd is not None:
                # Live cloth: hit-test against current particle positions
                # so the brush tracks what the user sees, not the static
                # mesh vertex layout. After any sim step, particles have
                # drooped away from their source vertices — using the
                # vertex map here pins random drooping particles.
                self._apply_pin_brush_to_live_particles(
                    hit_point, pin=not erasing,
                )
            else:
                # Pre-generate: mark vertices so the next Generate Cloth
                # pass turns them into fixed particles.
                affected = self._get_vertices_in_radius(hit_point, vertices)
                if erasing:
                    self._pin_vertices -= affected
                else:
                    self._pin_vertices |= affected

    def _get_triangles_in_radius(self, center: np.ndarray,
                                  vertices: np.ndarray,
                                  triangles: np.ndarray) -> set[int]:
        """Find triangle indices whose centroids are within brush radius.

        Triangles that belong to shapes marked as "Exclude from Painting"
        in the Cloth Area panel are skipped.
        """
        r_sq = self._brush_radius * self._brush_radius
        excluded = self._excluded_triangles()

        # Vectorized centroid distance test, then filter exclusions.
        centroids = vertices[triangles].mean(axis=1)
        diffs = centroids - center[np.newaxis, :]
        dists_sq = np.einsum("ij,ij->i", diffs, diffs)
        hit_idx = np.where(dists_sq <= r_sq)[0]
        if excluded:
            return set(int(i) for i in hit_idx if int(i) not in excluded)
        return set(int(i) for i in hit_idx)

    def _get_vertices_in_radius(self, center: np.ndarray,
                                 vertices: np.ndarray) -> set[int]:
        """Find vertex indices within brush radius.

        Vertices belonging to excluded shapes (Cloth Area panel) are skipped.
        """
        diffs = vertices - center[np.newaxis, :]
        dists_sq = np.sum(diffs * diffs, axis=1)
        r_sq = self._brush_radius * self._brush_radius
        hit = set(np.where(dists_sq <= r_sq)[0].tolist())
        excluded_verts = self._excluded_vertices()
        if excluded_verts:
            hit -= excluded_verts
        return hit

    def _apply_pin_brush_to_live_particles(
        self, hit_point: np.ndarray, pin: bool,
    ) -> None:
        """Find particles within brush radius and pin/unpin them on HKX.

        Uses current particle positions (not mesh vertices), so the
        brush always tracks what the user sees in the viewport, even
        after simulation has moved the particles.
        """
        scene = self.app.scene
        pd = scene.particle_data
        if pd is None or pd.positions is None:
            return

        positions = pd.positions
        if positions.shape[0] == 0:
            return

        diffs = positions - hit_point[np.newaxis, :]
        dists_sq = np.einsum("ij,ij->i", diffs, diffs)
        r_sq = float(self._brush_radius) * float(self._brush_radius)
        affected_ids = np.where(dists_sq <= r_sq)[0]
        if affected_ids.size == 0:
            return

        id_list = affected_ids.astype(np.intp).tolist()

        try:
            from creation_lib._native import havok_native
            new_blob = havok_native.cloth_set_particles_fixed(scene.blob, id_list, pin)
            scene.refresh_from_blob(new_blob)
        except Exception as e:
            _log.warning("Failed to update live pins: %s", e)
            return

        pd.is_fixed[affected_ids] = pin

        if self.app.particle_overlay is not None:
            self.app.particle_overlay.mark_dirty()

        pp = getattr(self.app, "preview_panel", None)
        if pp is not None:
            pp.solver = None
            pp.playing = False

    def _apply_pins_to_live_cloth(self, affected_vertices: set[int],
                                   pin: bool) -> None:
        """Map vertex indices to live particles and pin/unpin them on HKX.

        Guards against stale ``vertex_to_particle`` entries — the map can
        outlive a regenerate if the new cloth has fewer particles, so
        every mapped id is bounds-checked against the current particle
        array before touching HKX or the is_fixed mask.
        """
        scene = self.app.scene
        v2p = getattr(scene, "vertex_to_particle", None)
        if v2p is None:
            return

        pd = scene.particle_data
        n_particles = int(pd.positions.shape[0]) if pd is not None else 0
        if n_particles <= 0:
            return

        particle_ids: list[int] = []
        dropped = 0
        for v in affected_vertices:
            if not (0 <= v < len(v2p)):
                continue
            pi = int(v2p[v])
            if pi < 0:
                continue
            if pi >= n_particles:
                dropped += 1
                continue
            particle_ids.append(pi)

        if dropped:
            _log.warning(
                "Pin brush: dropped %d stale particle id(s) (max valid=%d) "
                "— vertex_to_particle map is out of sync with particle_data",
                dropped, n_particles - 1,
            )

        if not particle_ids:
            return

        try:
            from creation_lib._native import havok_native
            new_blob = havok_native.cloth_set_particles_fixed(scene.blob, particle_ids, pin)
            scene.refresh_from_blob(new_blob)
        except Exception as e:
            _log.warning("Failed to update live pins: %s", e)
            return

        import numpy as np
        idx_arr = np.array(particle_ids, dtype=np.intp)
        pd.is_fixed[idx_arr] = pin

        if self.app.particle_overlay is not None:
            self.app.particle_overlay.mark_dirty()

        pp = getattr(self.app, "preview_panel", None)
        if pp is not None:
            pp.solver = None
            pp.playing = False

    def _excluded_triangles(self) -> set[int]:
        cap = getattr(self.app, "cloth_area_panel", None)
        if cap is None:
            return set()
        return cap.get_excluded_triangle_set()

    def _excluded_vertices(self) -> set[int]:
        cap = getattr(self.app, "cloth_area_panel", None)
        if cap is None:
            return set()
        return cap.get_excluded_vertex_set()
