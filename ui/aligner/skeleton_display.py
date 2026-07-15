"""Skeleton and camera visualization for the scope aligner viewport.

Draws:
- Bone positions as small spheres/points connected by lines
- Camera position as a distinct marker
- Weapon bone highlighted
"""
from __future__ import annotations

import logging

import glm
import moderngl
import numpy as np

_log = logging.getLogger("aligner.skeleton_display")

# Colors (RGBA)
_BONE_COLOR = (0.8, 0.8, 0.2, 1.0)       # Yellow for bones
_CAMERA_COLOR = (0.2, 0.8, 1.0, 1.0)      # Cyan for camera
_WEAPON_COLOR = (1.0, 0.4, 0.2, 1.0)      # Orange for weapon
_LINE_COLOR = (0.5, 0.5, 0.5, 0.6)        # Gray for bone connections
_AIM_COLOR = (1.0, 0.2, 0.2, 0.8)         # Red for aim line

# Key bones to display (subset for clarity)
# Only show bones relevant to scope alignment (upper body + weapon chain)
# 1st person skeleton legs are vestigial — positioned incorrectly for display
_KEY_BONES = [
    "COM",
    "Spine1", "Spine2", "Chest",
    "RArm_Collarbone", "RArm_UpperArm", "RArm_ForeArm1",
    "RArm_ForeArm2", "RArm_ForeArm3", "RArm_Hand",
    "LArm_Collarbone", "LArm_UpperArm", "LArm_ForeArm1",
    "LArm_ForeArm2", "LArm_ForeArm3", "LArm_Hand",
    "Weapon", "WeaponLeft",
    "Camera",
]


class SkeletonDisplay:
    """Renders skeleton bones and camera as line/point overlay in the viewport."""

    MARKER_SIZE = 1.5  # radius of bone markers

    def __init__(self):
        self._vao: moderngl.VertexArray | None = None
        self._vbo: moderngl.Buffer | None = None
        self._color_vbo: moderngl.Buffer | None = None
        self._num_vertices = 0
        self.visible = True

        # Label data for imgui overlay: list of (name, wx, wy, wz, color)
        self._labels: list[tuple[str, float, float, float, tuple]] = []

    def rebuild(self, world_positions: dict, skeleton_data: dict,
                ctx: moderngl.Context, program: moderngl.Program):
        """Rebuild geometry from world-space bone positions.

        Args:
            world_positions: dict bone_name → (x, y, z) from animation_loader
            skeleton_data: dict with bone_names, parent_indices from skeleton XML
            ctx: ModernGL context
            program: Shader program with in_position + vColor attributes
        """
        self._release()
        self._labels = []

        if not world_positions:
            return

        bone_names = skeleton_data.get("bone_names", [])
        parent_indices = skeleton_data.get("parent_indices", [])

        positions = []  # float triples
        colors = []     # float quads

        def _add_cross(cx, cy, cz, size, color):
            """Add a 3D cross marker (6 line vertices)."""
            for dx, dy, dz in [(size, 0, 0), (0, size, 0), (0, 0, size)]:
                positions.extend([cx - dx, cy - dy, cz - dz])
                colors.extend(color)
                positions.extend([cx + dx, cy + dy, cz + dz])
                colors.extend(color)

        # Draw bone markers and connections
        for i, name in enumerate(bone_names):
            if name not in _KEY_BONES:
                continue
            if name not in world_positions:
                continue

            wx, wy, wz = [float(v) for v in world_positions[name]]

            # Choose color
            if name == "Camera":
                color = _CAMERA_COLOR
                marker_size = self.MARKER_SIZE * 2.0
            elif name == "Weapon":
                color = _WEAPON_COLOR
                marker_size = self.MARKER_SIZE * 1.5
            else:
                color = _BONE_COLOR
                marker_size = self.MARKER_SIZE

            _add_cross(wx, wy, wz, marker_size, color)

            self._labels.append((name, wx, wy, wz, color))

            # Draw line to parent bone
            parent_idx = parent_indices[i] if i < len(parent_indices) else -1
            if 0 <= parent_idx < len(bone_names):
                parent_name = bone_names[parent_idx]
                if parent_name in world_positions:
                    px, py, pz = [float(v) for v in world_positions[parent_name]]
                    positions.extend([wx, wy, wz])
                    colors.extend(_LINE_COLOR)
                    positions.extend([px, py, pz])
                    colors.extend(_LINE_COLOR)

        # Draw aim line from camera forward
        if "Camera" in world_positions:
            cx, cy, cz = [float(v) for v in world_positions["Camera"]]
            aim_length = 50.0
            # Aim along +Y (FO4 forward direction)
            positions.extend([cx, cy, cz])
            colors.extend(_AIM_COLOR)
            positions.extend([cx, cy + aim_length, cz])
            colors.extend(_AIM_COLOR)

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

    def render(self, program: moderngl.Program, mvp_tuple: tuple):
        """Render bone lines/markers."""
        if not self._vao or self._num_vertices == 0:
            return
        program["u_mvp"].value = mvp_tuple
        self._vao.render(moderngl.LINES)

    def draw_labels(self, vp_matrix, viewport_pos, viewport_size):
        """Draw bone name labels as imgui overlay (projected from 3D to screen)."""
        from imgui_bundle import imgui

        draw_list = imgui.get_window_draw_list()
        vp_w = viewport_size.x
        vp_h = viewport_size.y

        for name, wx, wy, wz, color in self._labels:
            # Project world → clip → screen
            clip = vp_matrix * glm.vec4(wx, wy, wz, 1.0)
            if clip.w <= 0:
                continue
            ndc_x = clip.x / clip.w
            ndc_y = clip.y / clip.w
            if abs(ndc_x) > 1.2 or abs(ndc_y) > 1.2:
                continue

            sx = viewport_pos.x + (ndc_x * 0.5 + 0.5) * vp_w
            sy = viewport_pos.y + (1.0 - (ndc_y * 0.5 + 0.5)) * vp_h

            # Draw label slightly offset from marker
            r, g, b, a = color
            col32 = imgui.get_color_u32(imgui.ImVec4(r, g, b, a))
            draw_list.add_text(imgui.ImVec2(sx + 8, sy - 6), col32, name)

    def _release(self):
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

    def destroy(self):
        self._release()
