"""3D brush cursor — renders a ring on the mesh surface that follows the mouse.

The ring is oriented to the surface normal at the hit point and scales
with the brush radius. Uses a simple solid-color line shader.
"""
from __future__ import annotations

import logging
import math

import moderngl
import numpy as np

_log = logging.getLogger("mesh_workspace.brush_cursor")

_RING_SEGMENTS = 64

# Inline shaders for the brush ring (simple solid-color lines)
_RING_VERT = """
#version 330 core
in vec3 in_position;
uniform mat4 u_mvp;
void main() {
    gl_Position = u_mvp * vec4(in_position, 1.0);
}
"""

_RING_FRAG = """
#version 330 core
uniform vec4 u_color;
out vec4 frag_color;
void main() {
    frag_color = u_color;
}
"""


class BrushCursor:
    """Renders a 3D ring on the mesh surface as a brush cursor."""

    def __init__(self, ctx: moderngl.Context):
        self.ctx = ctx
        self.program: moderngl.Program | None = None
        self.vao: moderngl.VertexArray | None = None
        self.vbo: moderngl.Buffer | None = None
        self._compile()
        self._build_ring()

    def _compile(self):
        try:
            self.program = self.ctx.program(
                vertex_shader=_RING_VERT,
                fragment_shader=_RING_FRAG,
            )
        except Exception as e:
            _log.error("Failed to compile brush cursor shaders: %s", e)
            self.program = None

    def _build_ring(self):
        """Build a unit circle in XY plane (Z=0) as a line loop."""
        if self.program is None:
            return

        verts = np.zeros((_RING_SEGMENTS, 3), dtype=np.float32)
        for i in range(_RING_SEGMENTS):
            angle = 2.0 * math.pi * i / _RING_SEGMENTS
            verts[i, 0] = math.cos(angle)
            verts[i, 1] = math.sin(angle)
            verts[i, 2] = 0.0

        self.vbo = self.ctx.buffer(verts.tobytes())
        self.vao = self.ctx.vertex_array(
            self.program,
            [(self.vbo, "3f", "in_position")],
        )

    def render(self, mvp_tuple: tuple, position: np.ndarray,
               normal: np.ndarray, radius: float,
               color: tuple[float, float, float, float] = (1.0, 1.0, 1.0, 0.9)):
        """Render the brush ring at the given position on the mesh surface.

        Args:
            mvp_tuple: View-projection matrix as tuple (model is identity at scene level).
            position: World-space center of the brush ring, shape (3,).
            normal: Surface normal at the hit point, shape (3,).
            radius: Brush radius in world units.
            color: RGBA color for the ring.
        """
        if self.program is None or self.vao is None:
            return

        import glm

        # Build a model matrix that:
        # 1. Scales the unit circle to brush radius
        # 2. Rotates from XY plane to align with surface normal
        # 3. Translates to hit position

        n = np.array(normal, dtype=np.float64)
        n_len = np.linalg.norm(n)
        if n_len < 1e-6:
            return
        n = n / n_len

        # Build rotation from Z-up (0,0,1) to surface normal
        up = np.array([0.0, 0.0, 1.0])
        dot = np.dot(up, n)

        if dot > 0.9999:
            # Already aligned
            rot = np.eye(3, dtype=np.float64)
        elif dot < -0.9999:
            # Opposite — rotate 180 around X
            rot = np.diag([1.0, -1.0, -1.0])
        else:
            axis = np.cross(up, n)
            axis /= np.linalg.norm(axis)
            angle = math.acos(np.clip(dot, -1.0, 1.0))
            # Rodrigues' rotation formula
            K = np.array([
                [0, -axis[2], axis[1]],
                [axis[2], 0, -axis[0]],
                [-axis[1], axis[0], 0],
            ])
            rot = np.eye(3) + math.sin(angle) * K + (1 - math.cos(angle)) * (K @ K)

        # Build 4x4 model matrix
        model = np.eye(4, dtype=np.float32)
        model[:3, :3] = (rot * radius).astype(np.float32)
        model[0, 3] = float(position[0])
        model[1, 3] = float(position[1])
        model[2, 3] = float(position[2])

        # Convert VP tuple to glm, multiply with model
        vp = glm.mat4(*mvp_tuple)
        m = glm.mat4(
            model[0, 0], model[1, 0], model[2, 0], model[3, 0],
            model[0, 1], model[1, 1], model[2, 1], model[3, 1],
            model[0, 2], model[1, 2], model[2, 2], model[3, 2],
            model[0, 3], model[1, 3], model[2, 3], model[3, 3],
        )
        final_mvp = vp * m
        mvp_flat = tuple(c for col in final_mvp for c in col)

        self.program["u_mvp"].value = mvp_flat
        self.program["u_color"].value = color

        # Slight offset toward camera to prevent z-fighting
        self.ctx.disable(moderngl.DEPTH_TEST)
        self.vao.render(moderngl.LINE_LOOP)
        self.ctx.enable(moderngl.DEPTH_TEST)
