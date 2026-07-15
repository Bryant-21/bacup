"""Pure-render half of the bone editor viewport.

Owns the SceneRenderer, FBO, skeleton overlay, weapon mesh sync, and
skinned mesh draw. NO interaction (no click-pick, no gizmo, no hotkeys).
The interaction layer (viewport_interact.py) calls into this module to
draw and into pose_session.py to mutate.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Optional

import glm
import moderngl
import numpy as np
from imgui_bundle import imgui

if TYPE_CHECKING:
    from creation_lib.bone_edit.bone_classifier import BoneCategory
    from creation_lib.bone_edit.skeleton import SkeletonManager

_log = logging.getLogger("bone_editor.viewport_render")


# Bone marker colors (RGBA) per category
_COLOR_LIMB = (0.45, 0.45, 0.45, 1.0)         # gray
_COLOR_IK_TIP = (0.20, 0.85, 1.00, 1.0)        # cyan
_COLOR_IK_POLE = (1.00, 0.30, 0.95, 1.0)       # magenta
_COLOR_MOUNT = (0.95, 0.85, 0.20, 1.0)         # yellow
_COLOR_SELECTED = (0.20, 1.00, 0.40, 1.0)      # green
_COLOR_LINE = (0.50, 0.50, 0.50, 0.6)          # bone connectors
_COLOR_POLE_TARGET = (1.00, 0.55, 0.10, 1.0)   # orange — IK pole swivel handles
_COLOR_POLE_LINE = (1.00, 0.55, 0.10, 0.35)    # pole-to-mid dotted connector

# Prefix used for pole handle names in the picking label list. Interact layer
# strips it to recover the mid bone name.
POLE_HANDLE_PREFIX = "pole:"


class SkeletonDisplay:
    """Renders skeleton bones as colored line/point overlay in the viewport.

    Reads bone positions from a per-frame world-pose dict computed by
    PoseSession. Coloring is by BoneCategory.
    """

    MARKER_SIZE = 1.5

    def __init__(self):
        self._vao: Optional[moderngl.VertexArray] = None
        self._vbo: Optional[moderngl.Buffer] = None
        self._color_vbo: Optional[moderngl.Buffer] = None
        self._num_vertices = 0
        self._labels: list[tuple[str, float, float, float, tuple]] = []
        self.visible = True

    def rebuild(
        self,
        skeleton: "SkeletonManager",
        world_pose: dict[str, tuple[np.ndarray, np.ndarray]],
        categories: dict[str, "BoneCategory"],
        edited_bones: set[str],
        selected_bone: Optional[str],
        hidden_bones: set[str],
        ctx: moderngl.Context,
        program: moderngl.Program,
        pole_targets: Optional[dict[str, np.ndarray]] = None,
    ) -> None:
        from creation_lib.bone_edit.bone_classifier import BoneCategory

        self._release()
        self._labels = []

        positions: list[float] = []
        colors: list[float] = []

        def _add_cross(cx, cy, cz, size, color):
            for dx, dy, dz in [(size, 0, 0), (0, size, 0), (0, 0, size)]:
                positions.extend([cx - dx, cy - dy, cz - dz])
                colors.extend(color)
                positions.extend([cx + dx, cy + dy, cz + dz])
                colors.extend(color)

        for i, name in enumerate(skeleton.bone_names):
            if name in hidden_bones:
                continue
            if name not in world_pose:
                continue
            wpos = world_pose[name][1]
            wx, wy, wz = float(wpos[0]), float(wpos[1]), float(wpos[2])

            # Color by category, with selected/edited overrides
            if name == selected_bone:
                color = _COLOR_SELECTED
                marker_size = self.MARKER_SIZE * 2.0
            else:
                cat = categories.get(name, BoneCategory.LIMB_SEGMENT)
                color = _category_color(cat)
                marker_size = self.MARKER_SIZE * (1.6 if name in edited_bones else 1.0)

            _add_cross(wx, wy, wz, marker_size, color)
            self._labels.append((name, wx, wy, wz, color))

            # Line to parent
            parent_idx = skeleton.parent_indices[i]
            if 0 <= parent_idx < skeleton.bone_count:
                parent_name = skeleton.bone_names[parent_idx]
                if parent_name in world_pose and parent_name not in hidden_bones:
                    p = world_pose[parent_name][1]
                    positions.extend([wx, wy, wz])
                    colors.extend(_COLOR_LINE)
                    positions.extend([float(p[0]), float(p[1]), float(p[2])])
                    colors.extend(_COLOR_LINE)

        # Pole targets: larger diamond markers + dotted connector to their mid bone.
        if pole_targets:
            pole_size = self.MARKER_SIZE * 2.2
            for mid_name, p in pole_targets.items():
                if mid_name in hidden_bones:
                    continue
                px, py, pz = float(p[0]), float(p[1]), float(p[2])
                handle_name = POLE_HANDLE_PREFIX + mid_name
                if handle_name == selected_bone:
                    color = _COLOR_SELECTED
                    size = pole_size * 1.4
                else:
                    color = _COLOR_POLE_TARGET
                    size = pole_size
                _add_cross(px, py, pz, size, color)
                # Add diagonals so the handle visually differs from bone crosses
                for dx, dy, dz in [
                    (size, size, 0), (size, -size, 0),
                    (size, 0, size), (size, 0, -size),
                    (0, size, size), (0, size, -size),
                ]:
                    positions.extend([px - dx, py - dy, pz - dz])
                    colors.extend(color)
                    positions.extend([px + dx, py + dy, pz + dz])
                    colors.extend(color)
                # Dotted connector to the mid bone
                if mid_name in world_pose:
                    mp = world_pose[mid_name][1]
                    mx, my, mz = float(mp[0]), float(mp[1]), float(mp[2])
                    segments = 8
                    for i in range(segments):
                        if i % 2:
                            continue
                        t0 = i / segments
                        t1 = (i + 1) / segments
                        positions.extend([
                            px + (mx - px) * t0,
                            py + (my - py) * t0,
                            pz + (mz - pz) * t0,
                        ])
                        colors.extend(_COLOR_POLE_LINE)
                        positions.extend([
                            px + (mx - px) * t1,
                            py + (my - py) * t1,
                            pz + (mz - pz) * t1,
                        ])
                        colors.extend(_COLOR_POLE_LINE)
                self._labels.append((handle_name, px, py, pz, color))

        if not positions:
            return

        pos_data = np.array(positions, dtype="f4")
        col_data = np.array(colors, dtype="f4")
        self._vbo = ctx.buffer(pos_data.tobytes())
        self._color_vbo = ctx.buffer(col_data.tobytes())
        self._num_vertices = len(positions) // 3
        self._vao = ctx.vertex_array(
            program,
            [
                (self._vbo, "3f", "in_position"),
                (self._color_vbo, "4f", "in_color"),
            ],
        )

    def render(self, program: moderngl.Program, mvp_tuple: tuple) -> None:
        if not self._vao or self._num_vertices == 0:
            return
        program["u_mvp"].value = mvp_tuple
        self._vao.render(moderngl.LINES)

    def project_labels(
        self, vp_matrix, viewport_pos, viewport_size,
    ) -> list[tuple[str, float, float, tuple]]:
        vp_w = viewport_size.x
        vp_h = viewport_size.y
        projected = []
        for name, wx, wy, wz, color in self._labels:
            clip = vp_matrix * glm.vec4(wx, wy, wz, 1.0)
            if clip.w <= 0:
                continue
            ndc_x = clip.x / clip.w
            ndc_y = clip.y / clip.w
            if abs(ndc_x) > 1.2 or abs(ndc_y) > 1.2:
                continue
            sx = viewport_pos.x + (ndc_x * 0.5 + 0.5) * vp_w
            sy = viewport_pos.y + (1.0 - (ndc_y * 0.5 + 0.5)) * vp_h
            projected.append((name, sx, sy, color))
        return projected

    def draw_labels(self, projected, visible_names: Optional[set[str]] = None) -> None:
        draw_list = imgui.get_window_draw_list()
        for name, sx, sy, color in projected:
            if visible_names is not None and name not in visible_names:
                continue
            if name.startswith(POLE_HANDLE_PREFIX):
                # Short label for pole handles so the viewport doesn't clutter
                label = "pole"
            else:
                label = name
            r, g, b, a = color
            col32 = imgui.get_color_u32(imgui.ImVec4(r, g, b, a))
            draw_list.add_text(imgui.ImVec2(sx + 8, sy - 6), col32, label)

    def _release(self) -> None:
        if self._vao:
            self._vao.release()
            self._vao = None
        if self._vbo:
            self._vbo.release()
            self._vbo = None
        if self._color_vbo:
            self._color_vbo.release()
            self._color_vbo = None
        self._num_vertices = 0


def _category_color(cat) -> tuple:
    from creation_lib.bone_edit.bone_classifier import BoneCategory
    return {
        BoneCategory.LIMB_SEGMENT: _COLOR_LIMB,
        BoneCategory.IK_TIP: _COLOR_IK_TIP,
        BoneCategory.IK_POLE: _COLOR_IK_POLE,
        BoneCategory.MOUNT: _COLOR_MOUNT,
    }.get(cat, _COLOR_LIMB)


class _GizmoTarget:
    """Lightweight wrapper to give a glm.mat4 the .world_transform attribute
    that GizmoManager expects."""
    def __init__(self, world_transform: glm.mat4):
        self.world_transform = world_transform
