"""Authoring panel — power-user cloth editing tools.

Interactive cloth tree (add/remove sim cloths, operators, constraints,
collidables, drag-drop operator ordering), viewport selection, capsule
tool, per-vertex painting brushes, and manual bake.
"""
from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

import numpy as np
from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.cloth_maker.cloth_maker_app import ClothMakerApp

_log = logging.getLogger("cloth_maker.authoring_panel")

# --- Per-vertex brush modes ---

_BRUSH_MODES = [
    ("mass", "Mass", 0.001, 0.5, "%.4f"),
    ("bend", "Bend Stiffness", 0.0, 1.0, "%.3f"),
    ("stretch", "Stretch Stiffness", 0.0, 1.0, "%.3f"),
    ("range", "Local Range", 0.0, 100.0, "%.1f"),
    ("fixed", "Pin/Unpin", 0.0, 1.0, ""),
]


@dataclass
class Selection:
    """Tracks selected particles in the viewport."""
    indices: set[int] = field(default_factory=set)

    def clear(self) -> None:
        self.indices.clear()

    def toggle(self, idx: int) -> None:
        if idx in self.indices:
            self.indices.discard(idx)
        else:
            self.indices.add(idx)

    def add(self, idx: int) -> None:
        self.indices.add(idx)

    @property
    def count(self) -> int:
        return len(self.indices)

    @property
    def empty(self) -> bool:
        return len(self.indices) == 0

    @property
    def pair(self) -> tuple[int, int] | None:
        """Return the two selected indices if exactly 2 are selected."""
        if len(self.indices) == 2:
            a, b = sorted(self.indices)
            return (a, b)
        return None


class AuthoringPanel:
    """Power-user cloth authoring panel."""

    def __init__(self, app: ClothMakerApp):
        self.app = app

        # Selection state
        self.selection = Selection()

        # Per-vertex brush
        self._brush_active: bool = False
        self._brush_mode_idx: int = 0  # index into _BRUSH_MODES
        self._brush_radius: float = 5.0
        self._brush_value: float = 0.1
        self._last_affected_count: int = 0

        # Capsule tool
        self._capsule_tool_active: bool = False
        self._capsule_bone_idx: int = 0
        self._capsule_radius: float = 3.0
        self._capsule_length: float = 10.0

        # Add capsule defaults
        self._add_capsule_bone: str = "bone_0"
        self._add_capsule_radius: float = 3.0
        self._add_capsule_start: list[float] = [0.0, 0.0, 0.0]
        self._add_capsule_end: list[float] = [0.0, 10.0, 0.0]

        # Interactive tree drag state
        self._drag_op_idx: int = -1

        # Bake state
        self._baking: bool = False
        self._bake_error: str = ""
        self._bake_success: str = ""

    @property
    def _has_cloth(self) -> bool:
        return self.app.scene.loaded

    def draw(self) -> None:
        visible, _ = imgui.begin("Authoring##cloth_maker")
        if not visible:
            imgui.end()
            return

        if not self._has_cloth:
            imgui.text_colored(
                imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                "No cloth loaded. Import a NIF first.",
            )
            imgui.spacing()
            imgui.text_disabled("Power-user authoring tools will appear here.")
            imgui.end()
            return

        self._draw_interactive_tree()
        imgui.spacing()
        self._draw_selection_section()
        imgui.spacing()
        self._draw_capsule_tool()
        imgui.spacing()
        self._draw_vertex_brush()
        imgui.spacing()
        self._draw_bake_section()

        imgui.end()

    # ------------------------------------------------------------------
    # Interactive cloth tree with add/remove/reorder
    # ------------------------------------------------------------------

    def _draw_interactive_tree(self) -> None:
        imgui.separator_text("Cloth Graph")

        cj = self.app.scene.cloth_json
        if cj is None:
            return

        leaf = imgui.TreeNodeFlags_.leaf.value
        default_open = imgui.TreeNodeFlags_.default_open.value

        # --- Operators ---
        operators = cj.get("operators", [])
        if imgui.tree_node_ex(f"Operators ({len(operators)})##auth", default_open):
            for i, op in enumerate(operators):
                cn = str(op.get("class_name", "")).replace("hcl", "")
                imgui.tree_node_ex(f"{i}: {cn}##auth_op_{i}", leaf)
                imgui.tree_pop()

            imgui.tree_pop()

        # --- Constraint sets ---
        scd = self.app.scene.active_sim_cloth
        if scd is not None:
            constraint_sets = scd.get("constraint_sets", [])
            if imgui.tree_node_ex(f"Constraints ({len(constraint_sets)})##auth"):
                for i, cs in enumerate(constraint_sets):
                    class_name = cs.get("class_name", "")
                    cn = class_name.replace("hcl", "").replace("ConstraintSet", "")
                    link_count = cs.get("link_count", len(cs.get("links", [])))

                    imgui.tree_node_ex(
                        f"{cn} ({link_count} links)##auth_cs_{i}", leaf,
                    )
                    imgui.tree_pop()

                imgui.tree_pop()

            # --- Collidables ---
            collidables = scd.get("collidables", [])
            if imgui.tree_node_ex(f"Collidables ({len(collidables)})##auth"):
                for i, col in enumerate(collidables):
                    label = col.get("name") or f"collidable_{i}"

                    imgui.tree_node_ex(f"{label}##auth_col_{i}", leaf)
                    imgui.tree_pop()

                imgui.tree_pop()

    # ------------------------------------------------------------------
    # Viewport selection
    # ------------------------------------------------------------------

    def _draw_selection_section(self) -> None:
        imgui.separator_text("Selection")

        if self.selection.empty:
            imgui.text_disabled(
                "Click particles in viewport to select.\n"
                "Shift+click to multi-select."
            )
        else:
            imgui.text(f"Selected: {self.selection.count} particles")
            indices_str = ", ".join(str(i) for i in sorted(self.selection.indices)[:20])
            if self.selection.count > 20:
                indices_str += "..."
            imgui.text_disabled(indices_str)

            # Show mass/radius info for selected particles
            pd = self.app.scene.particle_data
            if pd is not None:
                sel = sorted(self.selection.indices)
                valid = [i for i in sel if 0 <= i < len(pd.masses)]
                if valid:
                    masses = pd.masses[valid]
                    imgui.text(f"Mass: {masses.min():.4f} - {masses.max():.4f}")
                    fixed_count = sum(1 for i in valid if pd.is_fixed[i])
                    if fixed_count:
                        imgui.text(f"Fixed: {fixed_count}/{len(valid)}")

            if imgui.button("Clear Selection##auth"):
                self.selection.clear()

    def handle_viewport_click(self, particle_idx: int, shift_held: bool) -> None:
        """Handle a click on a particle in the viewport.

        Called from the viewport when the user clicks near a particle.

        Args:
            particle_idx: Index of the clicked particle (-1 if miss)
            shift_held: True if shift is held (multi-select)
        """
        if particle_idx < 0:
            if not shift_held:
                self.selection.clear()
            return

        if shift_held:
            self.selection.toggle(particle_idx)
        else:
            self.selection.clear()
            self.selection.add(particle_idx)

    def handle_viewport_drag(self, particle_idx: int,
                              delta: np.ndarray) -> None:
        """Handle dragging a selected particle in the viewport.

        Moves all selected particles by the given delta.

        Args:
            particle_idx: The particle being dragged
            delta: (3,) world-space displacement
        """
        if self.app.scene.particle_data is None:
            return
        if particle_idx not in self.selection.indices:
            return

        positions = self.app.scene.particle_data.positions
        for idx in self.selection.indices:
            if 0 <= idx < len(positions):
                positions[idx] += delta

    # ------------------------------------------------------------------
    # Capsule tool
    # ------------------------------------------------------------------

    def _draw_capsule_tool(self) -> None:
        imgui.separator_text("Capsule Editor")

        capsules = self.app.scene.capsules

        # --- Existing capsule editing ---
        if capsules:
            # Capsule selector
            capsule_labels = [
                f"{i}: {c.bone_name} (r={c.radius:.1f})" for i, c in enumerate(capsules)
            ]
            old_idx = self._capsule_bone_idx
            _, self._capsule_bone_idx = imgui.combo(
                "Capsule##capsule_tool", self._capsule_bone_idx, capsule_labels,
            )

            if self._capsule_bone_idx >= len(capsules):
                self._capsule_bone_idx = 0

            # Load capsule values when selection changes
            if self._capsule_bone_idx != old_idx or not self._capsule_tool_active:
                cap = capsules[self._capsule_bone_idx]
                self._capsule_radius = cap.radius
                self._capsule_length = float(np.linalg.norm(cap.end - cap.start))
                self._capsule_tool_active = True

            _, self._capsule_radius = imgui.slider_float(
                "Radius##capsule_tool", self._capsule_radius, 0.5, 30.0, "%.1f",
            )

            _, self._capsule_length = imgui.slider_float(
                "Length##capsule_tool", self._capsule_length, 1.0, 100.0, "%.1f",
            )

            if imgui.button("Apply Capsule Changes##auth", imgui.ImVec2(-1, 0)):
                self._apply_capsule_changes()

            imgui.same_line()
            if imgui.button("Remove##capsule_tool"):
                self._remove_capsule()

            imgui.text_disabled(
                "Viewport: click capsule to select, W=move, E=rotate, Esc=hide."
            )
        else:
            imgui.text_disabled("No capsules in cloth data.")

        # --- Add new capsule ---
        imgui.spacing()
        if imgui.tree_node_ex("Add Capsule##auth"):
            _, self._add_capsule_bone = imgui.input_text(
                "Bone Name##add_cap", self._add_capsule_bone, 64,
            )
            _, self._add_capsule_radius = imgui.slider_float(
                "Radius##add_cap", self._add_capsule_radius, 0.5, 30.0, "%.1f",
            )

            _, self._add_capsule_start[0] = imgui.slider_float(
                "Start X##add_cap", self._add_capsule_start[0], -200.0, 200.0, "%.1f",
            )
            _, self._add_capsule_start[1] = imgui.slider_float(
                "Start Y##add_cap", self._add_capsule_start[1], -200.0, 200.0, "%.1f",
            )
            _, self._add_capsule_start[2] = imgui.slider_float(
                "Start Z##add_cap", self._add_capsule_start[2], -200.0, 200.0, "%.1f",
            )

            _, self._add_capsule_end[0] = imgui.slider_float(
                "End X##add_cap", self._add_capsule_end[0], -200.0, 200.0, "%.1f",
            )
            _, self._add_capsule_end[1] = imgui.slider_float(
                "End Y##add_cap", self._add_capsule_end[1], -200.0, 200.0, "%.1f",
            )
            _, self._add_capsule_end[2] = imgui.slider_float(
                "End Z##add_cap", self._add_capsule_end[2], -200.0, 200.0, "%.1f",
            )

            if imgui.button("Add Capsule##auth_add", imgui.ImVec2(-1, 0)):
                self._add_capsule()

            imgui.tree_pop()

    # ------------------------------------------------------------------
    # Per-vertex painting brush
    # ------------------------------------------------------------------

    def _draw_vertex_brush(self) -> None:
        imgui.separator_text("Vertex Painting")

        changed, self._brush_active = imgui.checkbox(
            "Brush Active##auth_brush", self._brush_active,
        )
        if changed and self._brush_active:
            # Brushes are mutually exclusive — disable the Region brush.
            rp = getattr(self.app, "region_panel", None)
            if rp is not None and rp._brush_active:
                rp._brush_active = False

        if not self._brush_active:
            imgui.text_disabled("Enable brush to paint per-vertex properties.")
            return

        # Mode selector — includes stub modes; unsupported ones show a warning below instead of a slider
        mode_names = [m[1] for m in _BRUSH_MODES]
        _, self._brush_mode_idx = imgui.combo(
            "Property##auth_brush", self._brush_mode_idx, mode_names,
        )

        mode = _BRUSH_MODES[self._brush_mode_idx]
        mode_key = mode[0]

        # Warn about stub modes
        if mode_key in ("bend", "stretch"):
            imgui.text_colored(
                imgui.ImVec4(0.8, 0.6, 0.2, 1.0),
                "Not yet available — per-particle stiffness requires HKX extension",
            )
            return

        _, self._brush_radius = imgui.slider_float(
            "Radius##auth_brush", self._brush_radius, 1.0, 50.0, "%.1f",
        )

        if mode_key == "fixed":
            imgui.text_disabled("LMB: pin particles, Shift+LMB: unpin particles")
        else:
            _, self._brush_value = imgui.slider_float(
                "Value##auth_brush", self._brush_value,
                mode[2], mode[3], mode[4],
            )
            imgui.text_disabled("LMB: paint value, Shift+LMB: erase (set to default)")

        # Show affected count from last stroke
        if self._last_affected_count > 0:
            imgui.text(f"Last stroke: {self._last_affected_count} particles")

    def handle_brush_stroke(self, hit_point: np.ndarray,
                             erasing: bool = False) -> None:
        """Handle a brush stroke at the given world position.

        Finds particles within brush radius and sets their property.

        Args:
            hit_point: (3,) world-space brush center
            erasing: True if shift is held (reset to default)
        """
        if not self._brush_active:
            return
        if self.app.scene.particle_data is None:
            return

        positions = self.app.scene.particle_data.positions
        diffs = positions - hit_point[np.newaxis, :]
        dists_sq = np.sum(diffs * diffs, axis=1)
        r_sq = self._brush_radius * self._brush_radius
        affected = np.where(dists_sq <= r_sq)[0]

        if len(affected) == 0:
            self._last_affected_count = 0
            return

        mode_key = _BRUSH_MODES[self._brush_mode_idx][0]

        if mode_key == "fixed":
            # Pin/Unpin mode: LMB = pin, Shift+LMB = unpin
            pin = not erasing
            self.app.push_undo("Toggle fixed")
            try:
                from creation_lib._native import havok_native
                scene = self.app.scene
                new_blob = havok_native.cloth_set_particles_fixed(
                    scene.blob, [int(i) for i in affected.tolist()], pin,
                )
                scene.refresh_from_blob(new_blob)
                changed = len(affected)
                # Update scene particle_data.is_fixed to reflect immediately
                pd = scene.particle_data
                if pd is not None:
                    pd.is_fixed[affected] = pin
            except Exception as e:
                self.app.status_text = f"Pin error: {e}"
                _log.error("Failed to toggle fixed: %s", e, exc_info=True)
                return

            self._last_affected_count = len(affected)
            if self.app.particle_overlay is not None:
                self.app.particle_overlay.mark_dirty()
            self._invalidate_solver()

            action = "pinned" if pin else "unpinned"
            self.app.status_text = (
                f"Brush {action}: {changed} particles "
                f"({len(affected)} in radius)"
            )
            return

        if mode_key == "mass":
            target = self.app.scene.particle_data.masses
            default = 0.1
            hkx_setter = "set_particles_mass"
        elif mode_key == "range":
            target = self.app.scene.particle_data.radii
            default = 1.4
            hkx_setter = "set_particles_radius"
        else:
            # bend/stretch not supported yet
            return

        value = default if erasing else self._brush_value
        target[affected] = value
        self._last_affected_count = len(affected)

        # Persist to the blob so edits survive a preview reset.
        try:
            from creation_lib._native import havok_native
            scene = self.app.scene
            native_fn = getattr(havok_native, f"cloth_{hkx_setter}")
            new_blob, _ = native_fn(scene.blob, [int(i) for i in affected.tolist()], float(value))
            scene.refresh_from_blob(new_blob)
        except Exception as e:
            _log.warning("Failed to persist %s to HKX: %s", mode_key, e)

        # Mark particle overlay dirty so colors update immediately
        if self.app.particle_overlay is not None:
            self.app.particle_overlay.mark_dirty()
        self._invalidate_solver()

        action = "erase" if erasing else "paint"
        self.app.status_text = (
            f"Brush {action}: {len(affected)} particles, "
            f"{mode_key}={value:.4f}"
        )

    def _invalidate_solver(self) -> None:
        """Force the preview solver to rebuild next Play so edits take effect."""
        pp = getattr(self.app, "preview_panel", None)
        if pp is not None:
            pp.solver = None
            pp.playing = False

    # ------------------------------------------------------------------
    # Manual bake
    # ------------------------------------------------------------------

    def _draw_bake_section(self) -> None:
        imgui.separator()
        imgui.spacing()

        can_bake = self._has_cloth and not self._baking

        if not can_bake:
            imgui.begin_disabled()
        if imgui.button("Validate Bake (in-memory)##auth", imgui.ImVec2(-1, 30)):
            self._bake()
        if not can_bake:
            imgui.end_disabled()

        imgui.set_item_tooltip(
            "Sanity-check: re-serialize the current cloth graph to HKX bytes\n"
            "in memory. Does NOT modify any file.\n\n"
            "To write cloth into the NIF on disk, use File > Export NIF."
        )

        if self._bake_error:
            imgui.spacing()
            imgui.text_colored(
                imgui.ImVec4(1.0, 0.3, 0.3, 1.0),
                f"Bake error: {self._bake_error}",
            )

        if self._bake_success:
            imgui.spacing()
            imgui.text_colored(
                imgui.ImVec4(0.3, 1.0, 0.3, 1.0),
                self._bake_success,
            )

    # ------------------------------------------------------------------
    # Actions
    # ------------------------------------------------------------------

    def _add_capsule(self) -> None:
        """Add a new capsule collidable to the cloth data."""
        if not self._has_cloth:
            self.app.status_text = "No cloth loaded"
            return

        self.app.push_undo("Add capsule")
        try:
            from creation_lib._native import havok_native
            scene = self.app.scene

            start_4 = self._add_capsule_start + [0.0]
            end_4 = self._add_capsule_end + [0.0]
            new_blob, new_idx = havok_native.cloth_add_capsule(
                scene.blob,
                self._add_capsule_bone,
                float(self._add_capsule_radius),
                start_4,
                end_4,
            )

            scene.refresh_from_blob(new_blob)
            self._capsule_bone_idx = new_idx
            self._capsule_tool_active = False  # force reload on next draw
            self.app.status_text = (
                f"Added capsule {new_idx} ({self._add_capsule_bone}): "
                f"r={self._add_capsule_radius:.1f}"
            )
            _log.info("Added capsule %d: bone=%s, r=%.1f",
                      new_idx, self._add_capsule_bone, self._add_capsule_radius)
        except Exception as e:
            self.app.status_text = f"Add capsule error: {e}"
            _log.error("Failed to add capsule: %s", e, exc_info=True)

    def _remove_capsule(self) -> None:
        """Remove the currently selected capsule from the cloth data."""
        capsules = self.app.scene.capsules
        idx = self._capsule_bone_idx
        if idx < 0 or idx >= len(capsules):
            self.app.status_text = "No capsule selected"
            return

        self.app.push_undo("Remove capsule")
        cap = capsules[idx]
        try:
            from creation_lib._native import havok_native
            scene = self.app.scene
            new_blob = havok_native.cloth_remove_capsule(scene.blob, idx)
            scene.refresh_from_blob(new_blob)

            # Adjust selection index
            new_count = len(scene.capsules)
            if self._capsule_bone_idx >= new_count:
                self._capsule_bone_idx = max(0, new_count - 1)
            self._capsule_tool_active = False  # force reload on next draw

            self.app.status_text = f"Removed capsule {idx} ({cap.bone_name})"
            _log.info("Removed capsule %d (%s)", idx, cap.bone_name)
        except Exception as e:
            self.app.status_text = f"Remove capsule error: {e}"
            _log.error("Failed to remove capsule: %s", e, exc_info=True)

    def _apply_capsule_changes(self) -> None:
        """Modify the selected capsule's radius and length."""
        capsules = self.app.scene.capsules
        idx = self._capsule_bone_idx
        if idx < 0 or idx >= len(capsules):
            self.app.status_text = "No capsule selected"
            return

        self.app.push_undo("Modify capsule")
        cap = capsules[idx]
        try:
            from creation_lib._native import havok_native
            scene = self.app.scene
            blob = scene.blob

            blob = havok_native.cloth_set_capsule_radius(blob, idx, float(self._capsule_radius))

            # Update length by scaling endpoints along the capsule axis.
            direction = cap.end - cap.start
            cur_len = float(np.linalg.norm(direction))
            if cur_len > 1e-6:
                axis = direction / cur_len
                midpoint = (cap.start + cap.end) / 2.0
                half = self._capsule_length / 2.0
                new_start_w = midpoint - axis * half
                new_end_w = midpoint + axis * half
                local_s, local_e = scene.world_segment_to_bone_local(
                    cap.bone_name, new_start_w, new_end_w,
                )
                blob = havok_native.cloth_set_capsule_endpoints(
                    blob,
                    idx,
                    local_s.tolist() + [0.0],
                    local_e.tolist() + [0.0],
                )

            scene.refresh_from_blob(blob)
            self.app.status_text = (
                f"Updated capsule {idx} ({cap.bone_name}): "
                f"r={self._capsule_radius:.1f}, l={self._capsule_length:.1f}"
            )
            _log.info("Updated capsule %d: r=%.1f, l=%.1f",
                      idx, self._capsule_radius, self._capsule_length)
        except Exception as e:
            self.app.status_text = f"Capsule edit error: {e}"
            _log.error("Failed to edit capsule: %s", e)

    def _bake(self) -> None:
        """Report blob size as the 'bake' status (blob is always current)."""
        self._baking = True
        self._bake_error = ""
        self._bake_success = ""

        try:
            blob = self.app.scene.blob
            if blob is None:
                raise ValueError("No cloth data loaded")
            size_kb = len(blob) / 1024.0
            self._bake_success = f"Baked successfully ({size_kb:.1f} KB)"
            self.app.status_text = self._bake_success
            _log.info("Baked cloth data: %.1f KB", size_kb)
        except Exception as e:
            self._bake_error = str(e)
            _log.error("Bake failed: %s", e, exc_info=True)
        finally:
            self._baking = False
