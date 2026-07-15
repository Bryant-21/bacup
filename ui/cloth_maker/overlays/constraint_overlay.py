"""Constraint overlay — 3D line rendering for constraint links.

Color encodes constraint type. Alpha encodes stiffness.
"""
from __future__ import annotations

import logging
from typing import TYPE_CHECKING

import moderngl
import numpy as np

if TYPE_CHECKING:
    from ui.cloth_maker.cloth_scene import ConstraintLink, ParticleData

_log = logging.getLogger("cloth_maker.constraint_overlay")

_VERT = """
#version 330 core
in vec3 in_position;
in vec4 in_color;

uniform mat4 u_vp;

out vec4 v_color;

void main() {
    gl_Position = u_vp * vec4(in_position, 1.0);
    v_color = in_color;
}
"""

_FRAG = """
#version 330 core
in vec4 v_color;
out vec4 frag_color;
void main() {
    frag_color = v_color;
}
"""

# Color scheme per constraint type
_TYPE_COLORS = {
    "standard": (1.0, 0.9, 0.2),
    "stretch": (1.0, 0.9, 0.2),
    "bend": (0.2, 0.9, 0.9),
    "localrange": (0.9, 0.2, 0.9),
    "volume": (0.2, 0.9, 0.2),
    "unknown": (0.5, 0.5, 0.5),
}


class ConstraintOverlay:
    """Renders constraint links as colored 3D lines."""

    def __init__(self, ctx: moderngl.Context):
        self.ctx = ctx
        self.program: moderngl.Program | None = None
        self.vao: moderngl.VertexArray | None = None
        self.vbo: moderngl.Buffer | None = None
        self._line_count = 0
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
            _log.error("Failed to compile constraint overlay shaders: %s", e)
            self.program = None

    def _update(self, constraint_links: list[ConstraintLink],
                particle_data: ParticleData) -> None:
        """Rebuild VBO from constraint links."""
        if self.program is None or particle_data is None:
            return

        n_links = len(constraint_links)
        if n_links == 0:
            self._line_count = 0
            return

        positions = particle_data.positions

        # 2 vertices per link, 7 floats per vertex (x,y,z, r,g,b,a)
        buf = np.zeros((n_links * 2, 7), dtype=np.float32)

        for i, link in enumerate(constraint_links):
            a, b = link.particle_a, link.particle_b
            if a < 0 or b < 0 or a >= len(positions) or b >= len(positions):
                continue

            color = _TYPE_COLORS.get(link.constraint_type, _TYPE_COLORS["unknown"])
            alpha = max(0.2, min(1.0, link.stiffness))

            idx = i * 2
            buf[idx, :3] = positions[a]
            buf[idx, 3:6] = color
            buf[idx, 6] = alpha

            buf[idx + 1, :3] = positions[b]
            buf[idx + 1, 3:6] = color
            buf[idx + 1, 6] = alpha

        data = buf.tobytes()
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
            [(self.vbo, "3f 4f", "in_position", "in_color")],
        )
        self._line_count = n_links
        self._dirty = False

    def render(self, vp_tuple: tuple, constraint_links: list[ConstraintLink],
               particle_data: ParticleData,
               data_version: int = 0) -> None:
        """Render constraint lines. Updates VBO if data changed."""
        if self.program is None or particle_data is None:
            return

        if self._dirty or data_version != self._last_data_version:
            self._update(constraint_links, particle_data)
            self._last_data_version = data_version

        if self._line_count == 0 or self.vao is None:
            return

        self.program["u_vp"].value = vp_tuple

        # Render lines through mesh (depth test off) with blending
        self.ctx.disable(moderngl.DEPTH_TEST)
        self.ctx.enable(moderngl.BLEND)
        self.ctx.blend_func = (moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA)

        self.vao.render(moderngl.LINES, vertices=self._line_count * 2)

        self.ctx.enable(moderngl.DEPTH_TEST)
        self.ctx.disable(moderngl.BLEND)
