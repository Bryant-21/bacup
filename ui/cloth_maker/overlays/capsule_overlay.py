"""Capsule/sphere collision overlay — wireframe + semi-transparent fill.

Capsules rendered as oriented cylinders with hemispherical caps.
Spheres as icospheres.
"""
from __future__ import annotations

import logging
import math
from typing import TYPE_CHECKING

import moderngl
import numpy as np

if TYPE_CHECKING:
    from ui.cloth_maker.cloth_scene import CapsuleData, SphereData

_log = logging.getLogger("cloth_maker.capsule_overlay")

_VERT = """
#version 330 core
in vec3 in_position;
uniform mat4 u_mvp;
void main() {
    gl_Position = u_mvp * vec4(in_position, 1.0);
}
"""

_FRAG = """
#version 330 core
uniform vec4 u_color;
out vec4 frag_color;
void main() {
    frag_color = u_color;
}
"""

# Colors
_FILL_COLOR = (0.3, 0.5, 0.8, 0.25)
_WIRE_COLOR = (0.4, 0.6, 0.9, 0.8)

# Capsule mesh resolution
_LON_SEGS = 16
_LAT_SEGS = 8


def _generate_capsule_mesh(start: np.ndarray, end: np.ndarray, radius: float):
    """Generate capsule vertices and indices (triangles + line edges).

    Returns (tri_verts, wire_verts) as flat float32 arrays.
    """
    # Capsule axis
    axis = end - start
    length = np.linalg.norm(axis)
    if length < 1e-6:
        # Degenerate — render as sphere at start
        return _generate_sphere_mesh(start, radius)

    axis_norm = axis / length

    # Build a local coordinate frame
    up = np.array([0, 0, 1], dtype=np.float32)
    if abs(np.dot(axis_norm, up)) > 0.99:
        up = np.array([1, 0, 0], dtype=np.float32)
    right = np.cross(axis_norm, up)
    right /= np.linalg.norm(right)
    up = np.cross(right, axis_norm)

    tri_verts = []
    wire_verts = []

    def _point_on_cylinder(u_angle, t):
        """Point on cylinder surface. t=0 at start, t=1 at end."""
        c = math.cos(u_angle)
        s = math.sin(u_angle)
        offset = right * c * radius + up * s * radius
        base = start + axis * t
        return base + offset

    def _point_on_cap(u_angle, v_angle, at_end):
        """Point on hemispherical cap."""
        c_u = math.cos(u_angle)
        s_u = math.sin(u_angle)
        c_v = math.cos(v_angle)
        s_v = math.sin(v_angle)
        offset = (right * c_u * c_v + up * s_u * c_v) * radius
        if at_end:
            offset += axis_norm * s_v * radius
            base = end
        else:
            offset -= axis_norm * s_v * radius
            base = start
        return base + offset

    # Generate cylinder quads
    for i in range(_LON_SEGS):
        a0 = 2.0 * math.pi * i / _LON_SEGS
        a1 = 2.0 * math.pi * (i + 1) / _LON_SEGS

        p00 = _point_on_cylinder(a0, 0)
        p10 = _point_on_cylinder(a1, 0)
        p01 = _point_on_cylinder(a0, 1)
        p11 = _point_on_cylinder(a1, 1)

        # Two triangles per quad
        tri_verts.extend([p00, p10, p01, p10, p11, p01])
        # Wire edges
        wire_verts.extend([p00, p10, p00, p01])

    # Generate hemispherical caps
    half_segs = _LAT_SEGS // 2
    for at_end in (False, True):
        for j in range(half_segs):
            v0 = (math.pi / 2.0) * j / half_segs
            v1 = (math.pi / 2.0) * (j + 1) / half_segs

            for i in range(_LON_SEGS):
                a0 = 2.0 * math.pi * i / _LON_SEGS
                a1 = 2.0 * math.pi * (i + 1) / _LON_SEGS

                p00 = _point_on_cap(a0, v0, at_end)
                p10 = _point_on_cap(a1, v0, at_end)
                p01 = _point_on_cap(a0, v1, at_end)
                p11 = _point_on_cap(a1, v1, at_end)

                tri_verts.extend([p00, p10, p01, p10, p11, p01])
                if j == 0:
                    wire_verts.extend([p00, p10])
                if i % 4 == 0:
                    wire_verts.extend([p00, p01])

    if not tri_verts:
        return np.zeros(0, dtype=np.float32), np.zeros(0, dtype=np.float32)

    tri_arr = np.array(tri_verts, dtype=np.float32).reshape(-1, 3)
    wire_arr = np.array(wire_verts, dtype=np.float32).reshape(-1, 3)
    return tri_arr, wire_arr


def _generate_sphere_mesh(center: np.ndarray, radius: float):
    """Generate an icosphere-like sphere mesh at given center/radius.

    Returns (tri_verts, wire_verts).
    """
    tri_verts = []
    wire_verts = []

    for j in range(_LAT_SEGS):
        v0 = math.pi * j / _LAT_SEGS
        v1 = math.pi * (j + 1) / _LAT_SEGS

        for i in range(_LON_SEGS):
            a0 = 2.0 * math.pi * i / _LON_SEGS
            a1 = 2.0 * math.pi * (i + 1) / _LON_SEGS

            def _sp(a, v):
                return center + radius * np.array([
                    math.sin(v) * math.cos(a),
                    math.sin(v) * math.sin(a),
                    math.cos(v),
                ], dtype=np.float32)

            p00 = _sp(a0, v0)
            p10 = _sp(a1, v0)
            p01 = _sp(a0, v1)
            p11 = _sp(a1, v1)

            tri_verts.extend([p00, p10, p01, p10, p11, p01])
            if j == 0:
                wire_verts.extend([p00, p10])
            if i % 4 == 0:
                wire_verts.extend([p00, p01])

    if not tri_verts:
        return np.zeros(0, dtype=np.float32), np.zeros(0, dtype=np.float32)

    tri_arr = np.array(tri_verts, dtype=np.float32).reshape(-1, 3)
    wire_arr = np.array(wire_verts, dtype=np.float32).reshape(-1, 3)
    return tri_arr, wire_arr


class CapsuleOverlay:
    """Renders collision shapes as wireframe + semi-transparent fill."""

    def __init__(self, ctx: moderngl.Context):
        self.ctx = ctx
        self.program: moderngl.Program | None = None
        self._fill_vao: moderngl.VertexArray | None = None
        self._fill_vbo: moderngl.Buffer | None = None
        self._wire_vao: moderngl.VertexArray | None = None
        self._wire_vbo: moderngl.Buffer | None = None
        self._fill_count = 0
        self._wire_count = 0
        self._dirty = True
        self._last_data_version: int = -1
        self._compile()

    def _compile(self):
        try:
            self.program = self.ctx.program(
                vertex_shader=_VERT,
                fragment_shader=_FRAG,
            )
        except Exception as e:
            _log.error("Failed to compile capsule overlay shaders: %s", e)
            self.program = None

    def _update(self, capsules: list[CapsuleData],
                spheres: list[SphereData]) -> None:
        """Rebuild VBOs from capsule/sphere data."""
        if self.program is None:
            return

        all_tri = []
        all_wire = []

        for cap in capsules:
            tri, wire = _generate_capsule_mesh(cap.start, cap.end, cap.radius)
            if len(tri) > 0:
                all_tri.append(tri)
            if len(wire) > 0:
                all_wire.append(wire)

        for sph in spheres:
            tri, wire = _generate_sphere_mesh(sph.center, sph.radius)
            if len(tri) > 0:
                all_tri.append(tri)
            if len(wire) > 0:
                all_wire.append(wire)

        # Build fill VBO
        if all_tri:
            tri_data = np.concatenate(all_tri, axis=0)
            data = tri_data.tobytes()
            if self._fill_vbo is None:
                self._fill_vbo = self.ctx.buffer(data)
            else:
                self._fill_vbo.release()
                self._fill_vbo = self.ctx.buffer(data)
            if self._fill_vao is not None:
                self._fill_vao.release()
            self._fill_vao = self.ctx.vertex_array(
                self.program, [(self._fill_vbo, "3f", "in_position")],
            )
            self._fill_count = len(tri_data)
        else:
            self._fill_count = 0

        # Build wire VBO
        if all_wire:
            wire_data = np.concatenate(all_wire, axis=0)
            data = wire_data.tobytes()
            if self._wire_vbo is None:
                self._wire_vbo = self.ctx.buffer(data)
            else:
                self._wire_vbo.release()
                self._wire_vbo = self.ctx.buffer(data)
            if self._wire_vao is not None:
                self._wire_vao.release()
            self._wire_vao = self.ctx.vertex_array(
                self.program, [(self._wire_vbo, "3f", "in_position")],
            )
            self._wire_count = len(wire_data)
        else:
            self._wire_count = 0

        self._dirty = False

    def render(self, vp_tuple: tuple, capsules: list[CapsuleData],
               spheres: list[SphereData],
               data_version: int = 0) -> None:
        """Render collision shapes. Updates VBOs if data changed."""
        if self.program is None:
            return

        if self._dirty or data_version != self._last_data_version:
            self._update(capsules, spheres)
            self._last_data_version = data_version

        self.program["u_mvp"].value = vp_tuple

        # Fill pass: semi-transparent triangles, depth test off (show through mesh)
        if self._fill_count > 0 and self._fill_vao is not None:
            self.ctx.disable(moderngl.DEPTH_TEST)
            self.ctx.enable(moderngl.BLEND)
            self.ctx.blend_func = (moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA)

            self.program["u_color"].value = _FILL_COLOR
            self._fill_vao.render(moderngl.TRIANGLES, vertices=self._fill_count)

            self.ctx.disable(moderngl.BLEND)

        # Wire pass: lines, depth test off (shows through fill)
        if self._wire_count > 0 and self._wire_vao is not None:
            self.ctx.disable(moderngl.DEPTH_TEST)
            self.ctx.enable(moderngl.BLEND)
            self.ctx.blend_func = (moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA)

            self.program["u_color"].value = _WIRE_COLOR
            self._wire_vao.render(moderngl.LINES, vertices=self._wire_count)

            self.ctx.enable(moderngl.DEPTH_TEST)
            self.ctx.disable(moderngl.BLEND)
