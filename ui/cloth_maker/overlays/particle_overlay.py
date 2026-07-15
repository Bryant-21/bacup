"""Particle overlay — 3D point-sprite rendering for cloth particles.

Color encodes mass (blue=light, red=heavy). Point size encodes radius.
Pinned particles render as oversized bright yellow-red markers so they
are obvious regardless of the mass gradient behind them.
"""
from __future__ import annotations

import logging
from typing import TYPE_CHECKING

import moderngl
import numpy as np

if TYPE_CHECKING:
    from ui.cloth_maker.cloth_scene import ParticleData

_log = logging.getLogger("cloth_maker.particle_overlay")

_VERT = """
#version 330 core
in vec3 in_position;
in float in_mass;
in float in_radius;
in float in_fixed;

uniform mat4 u_vp;
uniform float u_point_scale;
uniform float u_pins_only;   // 1.0 = only render fixed particles
uniform float u_pin_size_boost;  // extra size multiplier for fixed particles

out float v_mass;
out float v_fixed;

void main() {
    gl_Position = u_vp * vec4(in_position, 1.0);

    float size = in_radius * u_point_scale;
    bool is_pin = in_fixed > 0.5;
    if (is_pin) {
        size = max(size * u_pin_size_boost, 14.0);
    }

    // When in pins-only pass, collapse non-pin particles to a single
    // pixel so we can discard their fragment cheaply.
    if (u_pins_only > 0.5 && !is_pin) {
        size = 1.0;
    }

    gl_PointSize = max(size, 4.0);
    v_mass = in_mass;
    v_fixed = in_fixed;
}
"""

_FRAG = """
#version 330 core
in float v_mass;
in float v_fixed;

uniform float u_pins_only;

out vec4 frag_color;

vec3 mass_color(float t) {
    // t in [0,1] normalized mass
    // blue -> cyan -> green -> yellow -> red
    float r = smoothstep(0.25, 0.75, t);
    float g = 1.0 - abs(t - 0.5) * 2.0;
    float b = 1.0 - smoothstep(0.0, 0.5, t);
    return vec3(r, g, b);
}

void main() {
    // Drop non-fixed particles entirely in the pins-only pass.
    bool is_pin = v_fixed > 0.5;
    if (u_pins_only > 0.5 && !is_pin) discard;

    // Discard fragments outside the point circle
    vec2 coord = gl_PointCoord * 2.0 - 1.0;
    float dist = dot(coord, coord);
    if (dist > 1.0) discard;

    vec3 col;
    if (is_pin) {
        // Bright red core, yellow ring, dark outer edge for contrast.
        if (dist < 0.35) {
            col = vec3(1.0, 0.15, 0.15);
        } else if (dist < 0.75) {
            col = vec3(1.0, 0.95, 0.1);
        } else {
            col = vec3(0.05, 0.05, 0.05);
        }
    } else {
        col = mass_color(v_mass);
    }

    // Slight edge fade on the outermost rim
    float alpha = 1.0 - smoothstep(0.9, 1.0, dist);
    frag_color = vec4(col, alpha);
}
"""


class ParticleOverlay:
    """Renders cloth particles as colored point sprites."""

    def __init__(self, ctx: moderngl.Context):
        self.ctx = ctx
        self.program: moderngl.Program | None = None
        self.vao: moderngl.VertexArray | None = None
        self.vbo: moderngl.Buffer | None = None
        self._count = 0
        self._dirty = True
        self._last_positions_id: int = 0
        self._last_masses_id: int = 0
        self._compile()

    def mark_dirty(self) -> None:
        """Force a VBO rebuild on next render (call after in-place data edits)."""
        self._dirty = True

    def _compile(self):
        try:
            self.program = self.ctx.program(
                vertex_shader=_VERT,
                fragment_shader=_FRAG,
            )
        except Exception as e:
            _log.error("Failed to compile particle overlay shaders: %s", e)
            self.program = None

    def update(self, particle_data: ParticleData) -> None:
        """Rebuild VBO from particle data."""
        if self.program is None or particle_data is None:
            return

        n = particle_data.positions.shape[0]
        if n == 0:
            self._count = 0
            return

        # Ensure all arrays are the same length as positions (solver can
        # replace positions with a differently-sized array during preview)
        masses = particle_data.masses
        radii = particle_data.radii
        is_fixed = particle_data.is_fixed
        if len(masses) != n:
            masses = np.zeros(n, dtype=np.float32)
        if len(radii) != n:
            radii = np.full(n, 1.0, dtype=np.float32)
        if len(is_fixed) != n:
            is_fixed = np.zeros(n, dtype=bool)

        # Normalize mass to [0, 1] for color mapping using a fixed absolute
        # scale. Using masses.max() caused every particle to read as red when
        # all masses were uniform (e.g. after erase resets them to default).
        _MASS_LO = 0.01
        _MASS_HI = 0.5
        norm_masses = np.clip(
            (masses - _MASS_LO) / (_MASS_HI - _MASS_LO), 0.0, 1.0,
        ).astype(np.float32)

        # Interleaved buffer: [x, y, z, mass, radius, is_fixed] per particle
        buf = np.zeros((n, 6), dtype=np.float32)
        buf[:, :3] = particle_data.positions
        buf[:, 3] = norm_masses
        buf[:, 4] = radii
        buf[:, 5] = is_fixed.astype(np.float32)

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
            [(self.vbo, "3f 1f 1f 1f", "in_position", "in_mass", "in_radius", "in_fixed")],
        )
        self._count = n
        self._dirty = False
        self._last_positions_id = id(particle_data.positions)
        self._last_masses_id = id(particle_data.masses)

    def render(self, vp_tuple: tuple, particle_data: ParticleData,
               show_pins: bool = True, pins_only: bool = False) -> None:
        """Render particles. Updates VBO if data changed.

        Args:
            vp_tuple: 4x4 view-projection matrix as a 16-tuple.
            particle_data: Scene particle state.
            show_pins: Whether pin markers are enabled (boosts size + color).
            pins_only: When True, render only the fixed particles (non-pins
                are discarded in the shader). Used to overlay pins on top
                of the mesh when the main particle overlay is hidden.
        """
        if self.program is None or particle_data is None:
            return

        if (self._dirty
                or id(particle_data.positions) != self._last_positions_id
                or id(particle_data.masses) != self._last_masses_id):
            self.update(particle_data)

        if self._count == 0 or self.vao is None:
            return

        self.program["u_vp"].value = vp_tuple
        self.program["u_point_scale"].value = 8.0
        self.program["u_pins_only"].value = 1.0 if pins_only else 0.0
        self.program["u_pin_size_boost"].value = 2.2 if show_pins else 1.0

        self.ctx.disable(moderngl.DEPTH_TEST)
        self.ctx.enable(moderngl.PROGRAM_POINT_SIZE)
        self.ctx.enable(moderngl.BLEND)
        self.ctx.blend_func = (moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA)

        self.vao.render(moderngl.POINTS, vertices=self._count)

        self.ctx.enable(moderngl.DEPTH_TEST)
        self.ctx.disable(moderngl.PROGRAM_POINT_SIZE)
        self.ctx.disable(moderngl.BLEND)
