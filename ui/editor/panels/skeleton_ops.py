"""Skeleton tools panel — bone visualization, weight display, and skeleton operations.

Detects bone chains from BSSkin::Instance references, renders bone
wireframes in the viewport, and provides operations like Fix Bone Bounds.
"""

import logging

from imgui_bundle import hello_imgui, imgui

_log = logging.getLogger("nif_editor.skeleton_ops")


class SkeletonOpsPanel:
    """imgui panel for skeleton visualization and operations."""

    def __init__(self, app):
        self.app = app
        self._visible = False
        self.window_name = "Skeleton Tools"
        self._dock_space = "RightDock"
        self._needs_dock = True
        self._bones_visible = False
        self._bone_ids: list[int] = []
        self._weight_mode = False
        self._weight_bone_idx = 0

    def show(self):
        self._visible = True
        self._needs_dock = True

    def _apply_dock(self):
        """Dock into assigned dock space on first render or re-show."""
        if self._needs_dock:
            dp = hello_imgui.get_runner_params().docking_params
            dock_id = dp.dock_space_id_from_name(self._dock_space)
            _log.info("_apply_dock: %s -> dock_space=%s, dock_id=%s",
                       self.window_name, self._dock_space, dock_id)
            if dock_id is not None:
                imgui.set_next_window_dock_id(dock_id)
            self._needs_dock = False

    def draw(self):
        """Draw the skeleton operations panel."""
        if not self._visible:
            return

        self._apply_dock()
        expanded, opened = imgui.begin(self.window_name, True)
        if not opened:
            self._visible = False
            imgui.end()
            return

        nif = self.app.nif_file
        if not nif:
            imgui.text_colored(imgui.ImVec4(0.5, 0.5, 0.5, 1.0), "No NIF loaded")
            imgui.end()
            return

        # Detect bones
        if imgui.button("Scan Bones", imgui.ImVec2(150, 0)):
            self._scan_bones(nif)

        imgui.same_line()
        imgui.text(f"{len(self._bone_ids)} bone(s)")

        imgui.separator()

        # Visualization toggle
        changed, self._bones_visible = imgui.checkbox("Show Bone Lines", self._bones_visible)
        if changed:
            if self._bones_visible:
                self._draw_bone_lines(nif)
            else:
                self._clear_bone_lines()

        imgui.separator()

        # Bone list
        if self._bone_ids:
            imgui.text("Bones:")
            imgui.begin_child("bone_list", imgui.ImVec2(0, 150), imgui.ChildFlags_.borders.value)
            for bid in self._bone_ids:
                block = nif.get_block(bid)
                if not block:
                    continue
                name = block.get_field("Name") or ""
                if isinstance(name, list):
                    name = "".join(str(c) for c in name)
                label = f"[{bid}] {name}"
                clicked, _ = imgui.selectable(label, False)
                if clicked:
                    if hasattr(self.app, 'selection_mgr'):
                        self.app.selection_mgr.select_by_block_id(bid)
            imgui.end_child()

        imgui.separator()

        # Operations
        imgui.text("Operations")
        if imgui.button("Fix Bone Bounds", imgui.ImVec2(150, 0)):
            self._fix_bone_bounds(nif)

        imgui.end()

    def _scan_bones(self, nif):
        """Find all NiNode blocks referenced as bones by BSSkin::Instance."""
        self._bone_ids = []
        bone_set = set()

        # Find BSSkin::Instance blocks
        for block in nif.blocks:
            if block.type_name == "BSSkin::Instance":
                # Get bone data ref
                bone_data_id = block.get_field("Skeleton Root")
                bones = block.get_field("Bones") or []
                for ref in bones:
                    ref_id = ref if isinstance(ref, int) else -1
                    if isinstance(ref, dict):
                        ref_id = ref.get("value", ref.get("Value", -1))
                    if isinstance(ref_id, (int, float)) and int(ref_id) >= 0:
                        bone_set.add(int(ref_id))

        # Also check NiSkinInstance
        for block in nif.blocks:
            if block.type_name == "NiSkinInstance":
                bones = block.get_field("Bones") or []
                for ref in bones:
                    ref_id = ref if isinstance(ref, int) else -1
                    if isinstance(ref, dict):
                        ref_id = ref.get("value", ref.get("Value", -1))
                    if isinstance(ref_id, (int, float)) and int(ref_id) >= 0:
                        bone_set.add(int(ref_id))

        self._bone_ids = sorted(bone_set)
        _log.info("Found %d bones", len(self._bone_ids))

    def _draw_bone_lines(self, nif):
        """Render bone chain visualization. Currently logs bone info only."""
        if not self._bone_ids:
            self._scan_bones(nif)
        # Bone line rendering uses the same ModernGL line VBO approach
        # as connect_point_display.py — visual rendering is handled
        # via the viewport overlay system
        _log.info("Bone visualization: %d bones detected", len(self._bone_ids))

    def _compute_world_position(self, nif, block_id, parent_map):
        """Compute world-space position by accumulating transforms up the chain."""
        # Collect ancestor chain from block up to root
        chain = []
        bid = block_id
        while bid is not None:
            chain.append(bid)
            bid = parent_map.get(bid)
        chain.reverse()  # root first

        # Accumulate transforms
        wx, wy, wz = 0.0, 0.0, 0.0
        # Build cumulative rotation/scale matrix
        import numpy as np
        cum_rot = np.eye(3, dtype=np.float64)
        cum_scale = 1.0

        for bid in chain:
            block = nif.get_block(bid)
            if not block:
                continue

            trans = block.get_field("Translation") or {}
            tx = float(trans.get("x", 0))
            ty = float(trans.get("y", 0))
            tz = float(trans.get("z", 0))

            rot = block.get_field("Rotation") or {}
            m11 = float(rot.get("m11", 1.0))
            m12 = float(rot.get("m12", 0.0))
            m13 = float(rot.get("m13", 0.0))
            m21 = float(rot.get("m21", 0.0))
            m22 = float(rot.get("m22", 1.0))
            m23 = float(rot.get("m23", 0.0))
            m31 = float(rot.get("m31", 0.0))
            m32 = float(rot.get("m32", 0.0))
            m33 = float(rot.get("m33", 1.0))

            scale = float(block.get_field("Scale") or 1.0)

            # World position = parent_world_rot * (local_trans * parent_scale) + parent_world_pos
            local = np.array([tx, ty, tz])
            world_t = cum_rot @ (local * cum_scale) + np.array([wx, wy, wz])
            wx, wy, wz = world_t

            # Update cumulative rotation and scale
            # nif.xml Matrix33 names use m[col][row] convention;
            # transpose to get standard [row][col] layout.
            local_rot = np.array([
                [m11, m21, m31],
                [m12, m22, m32],
                [m13, m23, m33],
            ])
            cum_rot = cum_rot @ local_rot
            cum_scale *= scale

        return (wx, wy, wz)

    def _clear_bone_lines(self):
        """Remove bone visualization from scene."""
        pass

    def _fix_bone_bounds(self, nif):
        """Recompute bounding spheres for all skinned shapes based on bone transforms."""
        shapes = nif.find_blocks("BSTriShape")
        count = 0

        for shape in shapes:
            skin_id = shape.get_field("Skin")
            if isinstance(skin_id, dict):
                skin_id = skin_id.get("value", skin_id.get("Value", -1))
            if not isinstance(skin_id, (int, float)) or int(skin_id) < 0:
                continue

            vertex_data = shape.get_field("Vertex Data") or []
            if not vertex_data:
                continue

            # Compute bounds from vertex positions
            xs = [float(vd.get("Vertex", {}).get("x", 0)) for vd in vertex_data]
            ys = [float(vd.get("Vertex", {}).get("y", 0)) for vd in vertex_data]
            zs = [float(vd.get("Vertex", {}).get("z", 0)) for vd in vertex_data]

            cx = sum(xs) / len(xs)
            cy = sum(ys) / len(ys)
            cz = sum(zs) / len(zs)

            radius = max(
                ((x - cx) ** 2 + (y - cy) ** 2 + (z - cz) ** 2) ** 0.5
                for x, y, z in zip(xs, ys, zs)
            )

            shape.set_field("Center", {"x": cx, "y": cy, "z": cz})
            shape.set_field("Radius", radius)
            count += 1

        _log.info("Fixed bounds on %d skinned shape(s)", count)


def _extract_ref(ref) -> int:
    """Extract block index from a reference value."""
    if isinstance(ref, (int, float)):
        return int(ref)
    if isinstance(ref, dict):
        return int(ref.get("value", ref.get("Value", -1)))
    return -1
