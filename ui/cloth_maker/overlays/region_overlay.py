"""Region overlay — renders painted cloth region triangles as a semi-transparent
colored overlay on the mesh surface.

Blue-ish for region triangles, orange for pin vertices (shown as highlighted
triangles touching pinned verts).
"""
from __future__ import annotations

import logging

import moderngl
import numpy as np

_log = logging.getLogger("cloth_maker.region_overlay")

_VERT = """
#version 330 core
in vec3 in_position;

uniform mat4 u_vp;

void main() {
    gl_Position = u_vp * vec4(in_position, 1.0);
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


class RegionOverlay:
    """Renders painted region triangles as a flat semi-transparent overlay."""

    def __init__(self, ctx: moderngl.Context):
        self.ctx = ctx
        self.program: moderngl.Program | None = None
        self.vbo: moderngl.Buffer | None = None
        self.vao: moderngl.VertexArray | None = None
        self._tri_count = 0
        self._compile()

    def _compile(self):
        try:
            self.program = self.ctx.program(
                vertex_shader=_VERT,
                fragment_shader=_FRAG,
            )
        except Exception as e:
            _log.error("Failed to compile region overlay shaders: %s", e)
            self.program = None

    def _rebuild_vbo(self, vertices: np.ndarray, triangles: np.ndarray,
                     indices: set[int]) -> int:
        """Build a VBO from the subset of triangles indicated by *indices*.

        Returns the number of triangles written.
        """
        if not indices:
            return 0

        # Collect vertex positions for the selected triangles
        idx_list = sorted(indices)
        # Filter to valid range
        max_tri = len(triangles)
        idx_list = [i for i in idx_list if 0 <= i < max_tri]
        if not idx_list:
            return 0

        tri_sel = triangles[idx_list]  # (N, 3) int indices
        # Flatten to vertex positions: (N*3, 3) float32
        flat_verts = vertices[tri_sel.ravel()].astype(np.float32)
        data = flat_verts.tobytes()

        if self.vbo is None:
            self.vbo = self.ctx.buffer(data)
        else:
            if self.vbo.size == len(data):
                self.vbo.write(data)
            else:
                self.vbo.release()
                self.vbo = self.ctx.buffer(data)

        if self.vao is not None:
            self.vao.release()
        self.vao = self.ctx.vertex_array(
            self.program,
            [(self.vbo, "3f", "in_position")],
        )
        return len(idx_list)

    def render(self, vp_tuple: tuple, vertices: np.ndarray,
               triangles: np.ndarray, region_indices: set[int],
               color: tuple[float, float, float, float] = (0.2, 0.5, 0.8, 0.35),
               ) -> None:
        """Render the given triangle indices as a colored overlay.

        Args:
            vp_tuple: Flattened 4x4 view-projection matrix.
            vertices: (V, 3) mesh vertex positions.
            triangles: (T, 3) triangle vertex-index array.
            region_indices: Set of triangle indices to highlight.
            color: RGBA color for the overlay.
        """
        if self.program is None:
            return
        if not region_indices or vertices is None or triangles is None:
            return

        self._tri_count = self._rebuild_vbo(vertices, triangles, region_indices)
        if self._tri_count == 0 or self.vao is None:
            return

        self.program["u_vp"].value = vp_tuple
        self.program["u_color"].value = color

        ctx = self.ctx
        ctx.enable(moderngl.BLEND)
        ctx.blend_func = (moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA)
        ctx.enable(moderngl.DEPTH_TEST)
        ctx.disable(moderngl.CULL_FACE)

        # Slight polygon offset to render on top of the mesh without z-fighting
        ctx.enable_direct(0x8037)  # GL_POLYGON_OFFSET_FILL
        ctx.polygon_offset = (-1.0, -1.0)

        self.vao.render(moderngl.TRIANGLES, vertices=self._tri_count * 3)

        ctx.polygon_offset = (0.0, 0.0)
        ctx.enable(moderngl.CULL_FACE)
        ctx.disable(moderngl.BLEND)
