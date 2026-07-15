"""Point light visualization for the NIF editor viewport.

Renders NiPointLight nodes as 3D markers (wireframe diamond + radius circle)
at each light's world position, with text labels showing name and properties.

Uses the same connect_point shader (per-vertex color lines).
"""
from __future__ import annotations
import math
import logging

import glm
import numpy as np
import moderngl

_log = logging.getLogger("nif_editor.light_display")


class LightDisplay:
    """Renders NiPointLight markers and labels in the 3D viewport."""

    ICON_SIZE = 2.0          # Diamond half-extent
    RADIUS_SEGMENTS = 32     # Circle resolution
    LIGHT_COLOR = (1.0, 0.95, 0.4, 1.0)      # Warm yellow for light icon
    RADIUS_COLOR = (1.0, 0.85, 0.3, 0.5)     # Dimmer yellow for radius circle
    SELECTED_COLOR = (0.2, 1.0, 0.6, 1.0)    # Green for selected
    LABEL_COLOR = (1.0, 0.95, 0.5, 1.0)
    LABEL_SELECTED_COLOR = (0.3, 1.0, 0.7, 1.0)

    def __init__(self, app=None):
        self.app = app
        self._vbo: moderngl.Buffer | None = None
        self._color_vbo: moderngl.Buffer | None = None
        self._vao: moderngl.VertexArray | None = None
        self._num_vertices = 0
        self._visible = True  # Show by default when lights exist
        self._needs_rebuild = False
        self._selected_light_block_id: int | None = None
        # Label data: (name, wx, wy, wz, is_selected, color_rgb)
        self._labels: list[tuple[str, float, float, float, bool, tuple]] = []
        # Extracted light data for the renderer/shader
        self.point_lights: list[dict] = []
        # Lightweight SceneNodes for gizmo + picking (one per light, no mesh)
        self.light_nodes: list = []  # list[SceneNode]

        if app and hasattr(app, 'selection_mgr'):
            app.selection_mgr.on_selection_changed(self._on_selection_changed)

    @property
    def visible(self) -> bool:
        return self._visible

    @visible.setter
    def visible(self, val: bool):
        self._visible = val
        if not val:
            self._release()
            self._labels = []
        else:
            self._needs_rebuild = True

    def _on_selection_changed(self, nif_id, block_id):
        """Highlight selected light block."""
        nif = self.app.nif_file if self.app else None
        if not nif or block_id is None:
            self._selected_light_block_id = None
            if self._visible and self.point_lights:
                self._needs_rebuild = True
            return

        block = nif.get_block(block_id)
        if block and _is_point_light(nif, block.type_name):
            self._selected_light_block_id = block_id
            self._needs_rebuild = True
        else:
            old = self._selected_light_block_id
            self._selected_light_block_id = None
            if old is not None:
                self._needs_rebuild = True

    def rebuild(self, nif, ctx: moderngl.Context, program: moderngl.Program,
                nif_id: str = ""):
        """Rebuild light icon geometry and extract light data from NIF."""
        self._release()
        self._labels = []
        self.point_lights = []

        if not nif:
            return

        # Build parent map for world transforms
        parent_map = {}
        for block in nif.blocks:
            if not nif.schema.is_subtype_of(block.type_name, "NiNode"):
                continue
            for field_name in ("Children", "Effects"):
                refs = block.get_field(field_name) or []
                for ref in refs:
                    ref_id = _extract_ref(ref)
                    if ref_id >= 0:
                        parent_map[ref_id] = block.block_id

        # Find all point light blocks
        light_blocks = [
            b for b in nif.blocks
            if _is_point_light(nif, b.type_name)
        ]

        if not light_blocks:
            self.light_nodes = []
            return

        self.light_nodes = []
        vertices = []
        colors = []

        for lb in light_blocks:
            world_pos = _compute_world_position(nif, lb.block_id, parent_map)
            wx, wy, wz = world_pos

            is_selected = (lb.block_id == self._selected_light_block_id)

            # Extract light properties
            diffuse = lb.get_field("Diffuse Color") or {}
            r = float(diffuse.get("r", 1.0))
            g = float(diffuse.get("g", 1.0))
            b = float(diffuse.get("b", 1.0))

            dimmer = float(lb.get_field("Dimmer") or 1.0)

            # FO4 stores the light radius in Specular Color (R=G=B=radius).
            # Fall back to attenuation-based estimate only if specular is absent
            # or looks like an actual color (all components <= 1).
            specular = lb.get_field("Specular Color") or {}
            spec_r = float(specular.get("r", 0.0))
            if spec_r > 1.0:
                radius = spec_r
            else:
                const_atten = float(lb.get_field("Constant Attenuation") or 0.0)
                linear_atten = float(lb.get_field("Linear Attenuation") or 1.0)
                quad_atten = float(lb.get_field("Quadratic Attenuation") or 0.0)
                radius = _estimate_radius(const_atten, linear_atten, quad_atten)

            # Apply dimmer to color
            light_rgb = (r * dimmer, g * dimmer, b * dimmer)

            # Store for shader use.
            # Use a smooth quadratic falloff fitted to the radius rather than
            # the raw NIF attenuation coefficients — FO4's engine interprets
            # them differently and they produce near-zero intensity in our
            # shader's classic 1/(c+l*d+q*d²) formula.
            self.point_lights.append({
                "block_id": lb.block_id,
                "position": world_pos,
                "color": light_rgb,
                "radius": radius,
                "const_atten": 1.0,
                "linear_atten": 0.0,
                "quad_atten": 1.0 / max(radius * radius, 1.0),
            })

            # Lightweight SceneNode for gizmo + picking (no mesh)
            from creation_lib.renderer.scene_renderer import SceneNode
            light_node = SceneNode(
                name=_get_string(lb, "Name") or f"Light_{lb.block_id}",
                block_id=lb.block_id,
                nif_id=nif_id,
            )
            light_node.world_transform = glm.translate(glm.mat4(1.0), glm.vec3(wx, wy, wz))
            light_node.bound_center = glm.vec3(wx, wy, wz)
            light_node.bound_radius = self.ICON_SIZE * 2.0  # small pick sphere
            self.light_nodes.append(light_node)

            # Icon color: use the light's own color tinted toward yellow, or green if selected
            if is_selected:
                icon_color = self.SELECTED_COLOR
                circle_color = (*self.SELECTED_COLOR[:3], 0.5)
            else:
                # Tint icon with light color (ensure visible against dark bg)
                brightness = max(r, g, b, 0.3)
                icon_color = (
                    min(r / brightness * 0.8 + 0.2, 1.0),
                    min(g / brightness * 0.8 + 0.2, 1.0),
                    min(b / brightness * 0.8 + 0.2, 1.0),
                    1.0,
                )
                circle_color = (*icon_color[:3], 0.4)

            # Diamond icon (octahedron wireframe - 12 edges = 24 vertices)
            sz = self.ICON_SIZE
            top = (wx, wy, wz + sz)
            bot = (wx, wy, wz - sz)
            pts = [
                (wx + sz, wy, wz),
                (wx, wy + sz, wz),
                (wx - sz, wy, wz),
                (wx, wy - sz, wz),
            ]
            # Edges from top to each side point, and bottom to each side point
            for i in range(4):
                p = pts[i]
                pn = pts[(i + 1) % 4]
                # Top to side
                vertices.extend(top)
                colors.extend(icon_color)
                vertices.extend(p)
                colors.extend(icon_color)
                # Bottom to side
                vertices.extend(bot)
                colors.extend(icon_color)
                vertices.extend(p)
                colors.extend(icon_color)
                # Side ring
                vertices.extend(p)
                colors.extend(icon_color)
                vertices.extend(pn)
                colors.extend(icon_color)

            # Radius circle (XY plane)
            if radius > 0:
                for i in range(self.RADIUS_SEGMENTS):
                    a0 = 2.0 * math.pi * i / self.RADIUS_SEGMENTS
                    a1 = 2.0 * math.pi * ((i + 1) % self.RADIUS_SEGMENTS) / self.RADIUS_SEGMENTS
                    vertices.extend([
                        wx + math.cos(a0) * radius,
                        wy + math.sin(a0) * radius,
                        wz,
                    ])
                    colors.extend(circle_color)
                    vertices.extend([
                        wx + math.cos(a1) * radius,
                        wy + math.sin(a1) * radius,
                        wz,
                    ])
                    colors.extend(circle_color)

            # Label
            name = _get_string(lb, "Name") or f"Light_{lb.block_id}"
            self._labels.append((name, wx, wy, wz, is_selected, light_rgb))

        if not vertices:
            return

        pos_data = np.array(vertices, dtype=np.float32)
        col_data = np.array(colors, dtype=np.float32)
        self._vbo = ctx.buffer(pos_data.tobytes())
        self._color_vbo = ctx.buffer(col_data.tobytes())
        self._num_vertices = len(vertices) // 3

        self._vao = ctx.vertex_array(program, [
            (self._vbo, "3f", "in_position"),
            (self._color_vbo, "4f", "in_color"),
        ])

    def render(self, program: moderngl.Program, mvp_tuple):
        """Draw light icon lines."""
        if not self._visible or not self._vao:
            return
        program["u_mvp"].value = mvp_tuple
        self._vao.render(moderngl.LINES)

    def draw_labels(self, vp_matrix, viewport_pos, viewport_size):
        """Draw light name labels as imgui overlay text."""
        if not self._visible or not self._labels:
            return

        from imgui_bundle import imgui
        import glm

        draw_list = imgui.get_window_draw_list()
        vp_x = viewport_pos.x
        vp_y = viewport_pos.y
        vp_w = viewport_size.x
        vp_h = viewport_size.y

        for name, wx, wy, wz, is_selected, light_rgb in self._labels:
            clip = vp_matrix * glm.vec4(wx, wy, wz, 1.0)
            if clip.w <= 0.001:
                continue

            ndc_x = clip.x / clip.w
            ndc_y = clip.y / clip.w
            ndc_z = clip.z / clip.w

            if ndc_z < -1.0 or ndc_z > 1.0:
                continue

            screen_x = vp_x + (ndc_x * 0.5 + 0.5) * vp_w
            screen_y = vp_y + (1.0 - (ndc_y * 0.5 + 0.5)) * vp_h

            # Offset label above the icon
            screen_y -= 20.0

            if is_selected:
                color = imgui.color_convert_float4_to_u32(
                    imgui.ImVec4(*self.LABEL_SELECTED_COLOR))
            else:
                color = imgui.color_convert_float4_to_u32(
                    imgui.ImVec4(*self.LABEL_COLOR))

            shadow_color = imgui.color_convert_float4_to_u32(
                imgui.ImVec4(0.0, 0.0, 0.0, 0.8))

            text_size = imgui.calc_text_size(name)
            tx = screen_x - text_size.x * 0.5
            ty = screen_y

            draw_list.add_text(imgui.ImVec2(tx + 1, ty + 1), shadow_color, name)
            draw_list.add_text(imgui.ImVec2(tx, ty), color, name)

    def _release(self):
        if self._vbo:
            self._vbo.release()
            self._vbo = None
        if self._color_vbo:
            self._color_vbo.release()
            self._color_vbo = None
        if self._vao:
            self._vao.release()
            self._vao = None
        self._num_vertices = 0

    def destroy(self):
        self._release()


def _is_point_light(nif, type_name: str) -> bool:
    """Check if a block type is a point light."""
    return type_name in ("NiPointLight", "BSLight") or nif.schema.is_subtype_of(type_name, "NiPointLight")


def _get_string(block, field_name: str) -> str | None:
    val = block.get_field(field_name)
    if val is None:
        return None
    if isinstance(val, str):
        return val if val else None
    if isinstance(val, list):
        return "".join(str(c) for c in val) or None
    return str(val)


def _extract_ref(ref) -> int:
    if isinstance(ref, (int, float)):
        return int(ref)
    if isinstance(ref, dict):
        return int(ref.get("value", ref.get("Value", -1)))
    return -1


def _estimate_radius(const_atten: float, linear_atten: float, quad_atten: float) -> float:
    """Estimate visible radius from attenuation coefficients.

    Solves for distance where 1/(c + l*d + q*d^2) = threshold (1%).
    Falls back to reasonable default if attenuation is zero/degenerate.
    """
    threshold = 0.01  # 1% intensity cutoff
    target = 1.0 / threshold  # = 100: we need c + l*d + q*d^2 = 100

    if quad_atten > 1e-6:
        # Quadratic: q*d^2 + l*d + (c - target) = 0
        a = quad_atten
        b = linear_atten
        c = const_atten - target
        disc = b * b - 4 * a * c
        if disc >= 0:
            return (-b + math.sqrt(disc)) / (2 * a)
    elif linear_atten > 1e-6:
        # Linear: l*d + c = target
        return (target - const_atten) / linear_atten

    return 50.0  # Default radius when attenuation is negligible


def _compute_world_position(nif, block_id, parent_map):
    """Compute world-space position by accumulating transforms up the chain."""
    chain = []
    bid = block_id
    while bid is not None:
        chain.append(bid)
        bid = parent_map.get(bid)
    chain.reverse()

    wx, wy, wz = 0.0, 0.0, 0.0
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

        scale = float(block.get_field("Scale") or 1.0)

        local = np.array([tx, ty, tz])
        world_t = cum_rot @ (local * cum_scale) + np.array([wx, wy, wz])
        wx, wy, wz = world_t

        rot = block.get_field("Rotation") or {}
        local_rot = np.array([
            [float(rot.get("m11", 1)), float(rot.get("m21", 0)), float(rot.get("m31", 0))],
            [float(rot.get("m12", 0)), float(rot.get("m22", 1)), float(rot.get("m32", 0))],
            [float(rot.get("m13", 0)), float(rot.get("m23", 0)), float(rot.get("m33", 1))],
        ])
        cum_rot = cum_rot @ local_rot
        cum_scale *= scale

    return (wx, wy, wz)
