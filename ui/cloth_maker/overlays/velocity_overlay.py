"""Velocity overlay — renders per-particle velocity arrows.

Color encodes speed (blue=slow, red=fast). Arrow length is proportional
to velocity magnitude. Only shown during simulation preview.
"""
from __future__ import annotations

import logging

import moderngl
import numpy as np

_log = logging.getLogger("cloth_maker.velocity_overlay")

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


def _speed_color(t: float) -> tuple[float, float, float]:
    """Map normalized speed [0,1] to blue->cyan->green->yellow->red."""
    r = max(0.0, min(1.0, (t - 0.25) * 2.0))
    g = max(0.0, min(1.0, 1.0 - abs(t - 0.5) * 2.0))
    b = max(0.0, min(1.0, 1.0 - t * 2.0))
    return (r, g, b)


class VelocityOverlay:
    """Renders per-particle velocity as colored line segments (arrows)."""

    def __init__(self, ctx: moderngl.Context):
        self.ctx = ctx
        self.program: moderngl.Program | None = None
        self.vao: moderngl.VertexArray | None = None
        self.vbo: moderngl.Buffer | None = None
        self._line_count = 0
        self._compile()

    def _compile(self):
        try:
            self.program = self.ctx.program(
                vertex_shader=_VERT,
                fragment_shader=_FRAG,
            )
        except Exception as e:
            _log.error("Failed to compile velocity overlay shaders: %s", e)
            self.program = None

    def update(self, positions: np.ndarray, velocities: np.ndarray,
               arrow_scale: float = 0.05) -> None:
        """Rebuild VBO from positions and velocities.

        Args:
            positions: (N, 3) float32 particle positions
            velocities: (N, 3) float32 particle velocities
            arrow_scale: visual length multiplier for velocity arrows
        """
        if self.program is None:
            return

        n = len(positions)
        if n == 0 or len(velocities) != n:
            self._line_count = 0
            return

        speeds = np.linalg.norm(velocities, axis=1)  # (N,)
        max_speed = speeds.max()
        if max_speed > 1e-6:
            norm_speeds = speeds / max_speed
        else:
            norm_speeds = np.zeros(n, dtype=np.float32)

        # Skip particles with negligible velocity
        active = speeds > 1e-4
        active_count = int(np.sum(active))
        if active_count == 0:
            self._line_count = 0
            return

        # 2 vertices per arrow, 7 floats per vertex (x,y,z, r,g,b,a)
        buf = np.zeros((active_count * 2, 7), dtype=np.float32)

        idx = 0
        for i in range(n):
            if not active[i]:
                continue
            t = norm_speeds[i]
            r, g, b = _speed_color(t)

            # Start at particle position
            buf[idx, :3] = positions[i]
            buf[idx, 3:6] = (r, g, b)
            buf[idx, 6] = 0.8

            # End at position + velocity * scale
            buf[idx + 1, :3] = positions[i] + velocities[i] * arrow_scale
            buf[idx + 1, 3:6] = (r, g, b)
            buf[idx + 1, 6] = 0.3  # fade at tip

            idx += 2

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
        self._line_count = active_count

    def render(self, vp_tuple: tuple, positions: np.ndarray,
               velocities: np.ndarray, arrow_scale: float = 0.05) -> None:
        """Render velocity arrows. Rebuilds VBO each frame during sim."""
        if self.program is None:
            return
        if positions is None or velocities is None:
            return

        self.update(positions, velocities, arrow_scale)

        if self._line_count == 0 or self.vao is None:
            return

        self.program["u_vp"].value = vp_tuple

        self.ctx.disable(moderngl.DEPTH_TEST)
        self.ctx.enable(moderngl.BLEND)
        self.ctx.blend_func = (moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA)

        self.vao.render(moderngl.LINES, vertices=self._line_count * 2)

        self.ctx.enable(moderngl.DEPTH_TEST)
        self.ctx.disable(moderngl.BLEND)
