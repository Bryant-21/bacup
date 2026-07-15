"""2D vector renderer for the SWF editor.

Tessellates filled paths to triangle meshes via earcut, renders
with ModernGL. Caches tessellations per shape -- invalidates on edit.
"""
from __future__ import annotations

import logging
from pathlib import Path
from dataclasses import dataclass, field

import glm
import moderngl
import numpy as np

from creation_lib.swf.types import RGBA, FillStyle, TWIPS_PER_PIXEL
from creation_lib.swf.shapes import (
    ShapeDef, ShapeRecord, StraightEdge, CurvedEdge, StyleChange, EndShape,
)

_log = logging.getLogger(__name__)
_SHADER_DIR = Path(__file__).parent / "shaders"


@dataclass
class TessellatedShape:
    """Cached tessellation of a shape for GPU rendering."""
    shape_id: int
    vao: moderngl.VertexArray | None = None
    vbo: moderngl.Buffer | None = None
    num_vertices: int = 0
    stroke_vao: moderngl.VertexArray | None = None
    stroke_vbo: moderngl.Buffer | None = None
    num_stroke_vertices: int = 0
    dirty: bool = True

    def release(self) -> None:
        if self.vao:
            self.vao.release()
        if self.vbo:
            self.vbo.release()
        if self.stroke_vao:
            self.stroke_vao.release()
        if self.stroke_vbo:
            self.stroke_vbo.release()


class SwfRenderer:
    """2D vector renderer using ModernGL."""

    _THUMB_POOL_SIZE = 3  # Number of FBOs in the thumbnail render pool

    def __init__(self, ctx: moderngl.Context):
        self.ctx = ctx
        self.fbo: moderngl.Framebuffer | None = None
        self.fbo_texture: moderngl.Texture | None = None
        self._fbo_depth: moderngl.Renderbuffer | None = None
        self.fbo_width = 0
        self.fbo_height = 0

        # Shader programs
        self.fill_prog = self._load_program("fill")
        self.stroke_prog = self._load_program("stroke")
        self.grid_prog = self._load_program("grid")

        # Tessellation cache
        self._cache: dict[int, TessellatedShape] = {}

        # Grid VAO (built on first use)
        self._grid_vao: moderngl.VertexArray | None = None
        self._grid_vbo: moderngl.Buffer | None = None
        self._canvas_border_vao: moderngl.VertexArray | None = None
        self._canvas_border_vbo: moderngl.Buffer | None = None

        # Thumbnail FBO pool: list of (fbo, texture, depth, size)
        self._thumb_fbos: list = []

    def _load_program(self, name: str) -> moderngl.Program:
        vert = (_SHADER_DIR / f"{name}.vert").read_text()
        frag = (_SHADER_DIR / f"{name}.frag").read_text()
        return self.ctx.program(vertex_shader=vert, fragment_shader=frag)

    def ensure_fbo(self, width: int, height: int) -> None:
        """Create or resize the FBO."""
        width = max(width, 1)
        height = max(height, 1)
        if self.fbo and self.fbo_width == width and self.fbo_height == height:
            return
        if self.fbo:
            self.fbo.release()
            self.fbo = None
        if self.fbo_texture:
            self.fbo_texture.release()
            self.fbo_texture = None
        if self._fbo_depth:
            self._fbo_depth.release()
            self._fbo_depth = None
        self.fbo_texture = self.ctx.texture((width, height), 4)
        self._fbo_depth = self.ctx.depth_renderbuffer((width, height))
        self.fbo = self.ctx.framebuffer(
            color_attachments=[self.fbo_texture],
            depth_attachment=self._fbo_depth,
        )
        self.fbo_width = width
        self.fbo_height = height

    def get_fbo_texture_id(self) -> int | None:
        if self.fbo_texture:
            return self.fbo_texture.glo
        return None

    def render(self, projection: glm.mat4, shapes: list[tuple[ShapeDef, glm.mat4]],
               bg_color: tuple[float, float, float] = (0.2, 0.2, 0.2),
               show_grid: bool = True,
               canvas_width: int = 550, canvas_height: int = 400) -> None:
        """Render all visible shapes to the FBO."""
        if not self.fbo:
            return

        self.fbo.use()
        self.ctx.clear(*bg_color, 1.0)
        self.ctx.enable(moderngl.BLEND)
        self.ctx.blend_func = (
            moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA,
        )

        proj_bytes = _mat4_to_bytes(projection)

        # Draw canvas background (#333333)
        self._draw_canvas_rect(proj_bytes, canvas_width, canvas_height)

        # Draw shapes (bottom to top)
        for shape, transform in shapes:
            self._draw_shape(shape, proj_bytes)

        # Grid overlay
        if show_grid:
            self._draw_grid(proj_bytes, canvas_width, canvas_height)

        self.ctx.disable(moderngl.BLEND)
        self.ctx.screen.use()

    def tessellate_shape(self, shape: ShapeDef) -> TessellatedShape:
        """Tessellate a shape into GPU-ready triangles."""
        cached = self._cache.get(shape.shape_id)
        if cached and not cached.dirty:
            return cached

        if cached:
            cached.release()

        ts = TessellatedShape(shape_id=shape.shape_id)
        vertices = _tessellate_fills(shape)

        if len(vertices) > 0:
            data = np.array(vertices, dtype="f4")
            ts.vbo = self.ctx.buffer(data.tobytes())
            ts.vao = self.ctx.vertex_array(
                self.fill_prog,
                [(ts.vbo, "2f 4f", "in_position", "in_color")],
            )
            ts.num_vertices = len(vertices) // 6  # 2 pos + 4 color per vertex

        ts.dirty = False
        self._cache[shape.shape_id] = ts
        return ts

    def invalidate_shape(self, shape_id: int) -> None:
        """Mark a shape's tessellation as dirty."""
        cached = self._cache.get(shape_id)
        if cached:
            cached.dirty = True

    def _draw_shape(self, shape: ShapeDef, proj_bytes: bytes) -> None:
        ts = self.tessellate_shape(shape)
        if ts.vao and ts.num_vertices > 0:
            self.fill_prog["u_projection"].write(proj_bytes)
            ts.vao.render(moderngl.TRIANGLES, vertices=ts.num_vertices)

    def _draw_canvas_rect(self, proj_bytes: bytes, w: int, h: int) -> None:
        """Draw the SWF canvas background rectangle."""
        if not self._canvas_border_vbo:
            # Two triangles for a filled rect
            verts = np.array([
                0, 0,  0.2, 0.2, 0.2, 1.0,
                w, 0,  0.2, 0.2, 0.2, 1.0,
                w, h,  0.2, 0.2, 0.2, 1.0,
                0, 0,  0.2, 0.2, 0.2, 1.0,
                w, h,  0.2, 0.2, 0.2, 1.0,
                0, h,  0.2, 0.2, 0.2, 1.0,
            ], dtype="f4")
            self._canvas_border_vbo = self.ctx.buffer(verts.tobytes())
            self._canvas_border_vao = self.ctx.vertex_array(
                self.fill_prog,
                [(self._canvas_border_vbo, "2f 4f", "in_position", "in_color")],
            )
        self.fill_prog["u_projection"].write(proj_bytes)
        self._canvas_border_vao.render(moderngl.TRIANGLES)

    def _draw_grid(self, proj_bytes: bytes, w: int, h: int) -> None:
        """No-op; grid overlay is not implemented."""
        pass

    def cleanup(self) -> None:
        """Release all GPU resources."""
        for ts in self._cache.values():
            ts.release()
        self._cache.clear()
        if self.fbo:
            self.fbo.release()
            self.fbo = None
        if self.fbo_texture:
            self.fbo_texture.release()
            self.fbo_texture = None
        if self._fbo_depth:
            self._fbo_depth.release()
            self._fbo_depth = None
        self.fbo_width = 0
        self.fbo_height = 0
        for fbo, tex, depth, _ in self._thumb_fbos:
            fbo.release()
            tex.release()
            depth.release()
        self._thumb_fbos.clear()

    def _get_thumb_fbo(
        self, size: int
    ) -> tuple[moderngl.Framebuffer, moderngl.Texture]:
        """Get or create a pooled FBO for thumbnail rendering at given size.

        Pools up to _THUMB_POOL_SIZE FBOs per size. Returns (fbo, texture).
        Caller must not release these — the pool owns them.
        """
        # Find an existing pool entry matching this size
        for fbo, tex, depth, s in self._thumb_fbos:
            if s == size:
                return fbo, tex

        # Create a new one (up to pool cap — evict oldest if full)
        if len(self._thumb_fbos) >= self._THUMB_POOL_SIZE:
            old_fbo, old_tex, old_depth, _ = self._thumb_fbos.pop(0)
            old_fbo.release(); old_tex.release(); old_depth.release()

        tex = self.ctx.texture((size, size), 4)
        depth = self.ctx.depth_renderbuffer((size, size))
        fbo = self.ctx.framebuffer(color_attachments=[tex], depth_attachment=depth)
        self._thumb_fbos.append((fbo, tex, depth, size))
        return fbo, tex

    def render_thumbnail_vertices(
        self,
        vertex_array: list[float],
        size: int,
    ) -> tuple[int, bytes]:
        """Render a pre-tessellated vertex array to an offscreen FBO.

        Args:
            vertex_array: Flat [x, y, r, g, b, a, ...] in tile-local px coords
                          (already scaled to fit within size×size, from ThumbnailLoader)
            size: Tile size in pixels (64, 96, or 128)

        Returns:
            (texture_glo, png_bytes): GL texture object name + raw PNG bytes
        """
        import struct
        import numpy as np
        from PIL import Image
        import io

        fbo, tex = self._get_thumb_fbo(size)
        fbo.use()
        self.ctx.clear(0.15, 0.15, 0.15, 1.0)
        self.ctx.enable(moderngl.BLEND)
        self.ctx.blend_func = (moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA)

        if vertex_array:
            # Orthographic projection: pixel coords → clip space
            # Maps [0, size] → [-1, 1], Y flipped for OpenGL convention
            l, r, b, t = 0.0, float(size), float(size), 0.0
            proj = glm.ortho(l, r, b, t, -1.0, 1.0)
            proj_bytes = _mat4_to_bytes(proj)

            data = np.array(vertex_array, dtype="f4")
            vbo = self.ctx.buffer(data.tobytes())
            vao = self.ctx.vertex_array(
                self.fill_prog,
                [(vbo, "2f 4f", "in_position", "in_color")],
            )
            self.fill_prog["u_projection"].write(proj_bytes)
            n_verts = len(vertex_array) // 6
            vao.render(moderngl.TRIANGLES, vertices=n_verts)
            vao.release()
            vbo.release()

        self.ctx.disable(moderngl.BLEND)
        self.ctx.screen.use()

        # Read back pixels (OpenGL stores rows bottom-up; flip for PNG)
        raw = fbo.read(components=4)
        img = Image.frombytes("RGBA", (size, size), raw)
        img = img.transpose(Image.FLIP_TOP_BOTTOM)
        buf = io.BytesIO()
        img.save(buf, format="PNG")
        png_bytes = buf.getvalue()

        return tex.glo, png_bytes


def _tessellate_fills(shape: ShapeDef) -> list[float]:
    """Tessellate filled subpaths to flat vertex array [x, y, r, g, b, a, ...].

    Uses even-odd fill rule: groups rings by fill style, then lets pyclipper's
    PolyTree nesting depth distinguish outer contours from holes before
    triangulating each with earcut, so eyes/mouth/detail cutouts render
    correctly.
    """
    from earcut.earcut import earcut as _earcut

    scale = 1.0 / TWIPS_PER_PIXEL
    active_fills: list[FillStyle] = list(shape.fill_styles)

    # Phase 1: walk records, collect (ring, fill_style) pairs
    rings: list[tuple[list[tuple[float, float]], FillStyle | None]] = []
    current_path: list[tuple[float, float]] = []
    cx, cy = 0.0, 0.0
    current_fill: FillStyle | None = None

    def _close_ring():
        nonlocal current_path
        if len(current_path) >= 3 and current_fill and current_fill.color:
            rings.append((current_path, current_fill))
        current_path = []

    for rec in shape.records:
        if isinstance(rec, StyleChange):
            if rec.has_move:
                _close_ring()
                cx = rec.move_x * scale
                cy = rec.move_y * scale
                current_path.append((cx, cy))

            if rec.new_fill_styles is not None:
                active_fills = list(rec.new_fill_styles)

            if rec.fill1 is not None and rec.fill1 > 0:
                idx = rec.fill1 - 1
                if idx < len(active_fills):
                    current_fill = active_fills[idx]
            elif rec.fill0 is not None and rec.fill0 > 0:
                idx = rec.fill0 - 1
                if idx < len(active_fills):
                    current_fill = active_fills[idx]

        elif isinstance(rec, StraightEdge):
            cx += rec.dx * scale
            cy += rec.dy * scale
            current_path.append((cx, cy))

        elif isinstance(rec, CurvedEdge):
            segments = _subdivide_quadratic(
                cx, cy,
                cx + rec.cx * scale, cy + rec.cy * scale,
                cx + (rec.cx + rec.ax) * scale, cy + (rec.cy + rec.ay) * scale,
            )
            for px, py in segments[1:]:
                current_path.append((px, py))
            cx = cx + (rec.cx + rec.ax) * scale
            cy = cy + (rec.cy + rec.ay) * scale

        elif isinstance(rec, EndShape):
            _close_ring()

    if not rings:
        return []

    # Phase 2: group rings by fill style, tessellate with holes
    all_vertices: list[float] = []

    # Group by fill style identity (same color = same group for even-odd)
    fill_groups: dict[int, list[tuple[list[tuple[float, float]], FillStyle]]] = {}
    for ring, fill in rings:
        # Key by fill color to group same-fill rings
        key = id(fill) if fill else 0
        # Use color hash for grouping (different FillStyle objects with same color)
        if fill and fill.color:
            key = (fill.color.r, fill.color.g, fill.color.b, fill.color.a)
        fill_groups.setdefault(key, []).append((ring, fill))

    for group in fill_groups.values():
        if not group:
            continue
        fill = group[0][1]
        if not fill or not fill.color:
            continue

        r = fill.color.r / 255.0
        g = fill.color.g / 255.0
        b = fill.color.b / 255.0
        a = fill.color.a / 255.0

        group_rings = [ring for ring, _ in group]
        _tessellate_even_odd(group_rings, r, g, b, a, all_vertices, _earcut)

    return all_vertices


def _tessellate_even_odd(
    group_rings: list[list[tuple[float, float]]],
    r: float, g: float, b: float, a: float,
    out: list[float],
    earcut_fn,
) -> None:
    """Tessellate rings with even-odd fill using pyclipper boolean XOR.

    SWF uses even-odd fill: overlapping same-fill regions cancel out.
    Pyclipper CT_UNION with PFT_EVENODD correctly handles this for any
    configuration of overlapping, nested, or adjacent rings.
    """
    if len(group_rings) == 1:
        ring = group_rings[0]
        flat = [v for pt in ring for v in pt]
        indices = earcut_fn(flat)
        for idx in indices:
            out.extend([ring[idx][0], ring[idx][1], r, g, b, a])
        return

    import pyclipper

    pc = pyclipper.Pyclipper()
    for ring in group_rings:
        try:
            pc.AddPath(
                pyclipper.scale_to_clipper(ring),
                pyclipper.PT_SUBJECT, True,
            )
        except pyclipper.ClipperException:
            continue

    try:
        tree = pc.Execute2(
            pyclipper.CT_UNION,
            pyclipper.PFT_EVENODD,
            pyclipper.PFT_EVENODD,
        )
    except pyclipper.ClipperException:
        # Fallback: tessellate each ring independently
        for ring in group_rings:
            if len(ring) < 3:
                continue
            flat = [v for pt in ring for v in pt]
            indices = earcut_fn(flat)
            for idx in indices:
                out.extend([ring[idx][0], ring[idx][1], r, g, b, a])
        return

    # Walk the PolyTree: outer contours at even depth, holes at odd depth
    _tessellate_polytree_node(tree, r, g, b, a, out, earcut_fn)


def _tessellate_polytree_node(node, r, g, b, a, out, earcut_fn):
    """Recursively tessellate a pyclipper PolyTree with earcut."""
    import pyclipper

    for child in node.Childs:
        # Child is an outer contour
        outer = pyclipper.scale_from_clipper(child.Contour)
        if len(outer) < 3:
            continue

        flat: list[float] = [v for pt in outer for v in pt]
        all_points: list[tuple[float, float]] = [(p[0], p[1]) for p in outer]
        hole_indices: list[int] = []

        # Child's children are holes
        for hole_node in child.Childs:
            hole = pyclipper.scale_from_clipper(hole_node.Contour)
            if len(hole) < 3:
                continue
            hole_indices.append(len(all_points))
            for pt in hole:
                all_points.append((pt[0], pt[1]))
                flat.extend([pt[0], pt[1]])

            # Hole's children are nested outers — recurse
            if hole_node.Childs:
                _tessellate_polytree_node(hole_node, r, g, b, a, out, earcut_fn)

        indices = earcut_fn(flat, hole_indices if hole_indices else None)
        for idx in indices:
            out.extend([all_points[idx][0], all_points[idx][1], r, g, b, a])


def _subdivide_quadratic(
    x0: float, y0: float,
    cx: float, cy: float,
    x1: float, y1: float,
    depth: int = 0,
    max_depth: int = 5,
) -> list[tuple[float, float]]:
    """Adaptively subdivide quadratic bezier to line segments."""
    if depth >= max_depth:
        return [(x0, y0), (x1, y1)]

    # Midpoint
    mx = (x0 + 2 * cx + x1) / 4
    my = (y0 + 2 * cy + y1) / 4

    # Flatness test: distance from midpoint to line (x0,y0)->(x1,y1)
    dx = x1 - x0
    dy = y1 - y0
    if dx == 0 and dy == 0:
        return [(x0, y0), (x1, y1)]

    d = abs((mx - x0) * dy - (my - y0) * dx) / (dx * dx + dy * dy) ** 0.5
    if d < 0.5:  # half pixel tolerance
        return [(x0, y0), (x1, y1)]

    # Split at t=0.5
    c0x = (x0 + cx) / 2;  c0y = (y0 + cy) / 2
    c1x = (cx + x1) / 2;  c1y = (cy + y1) / 2
    mmx = (c0x + c1x) / 2; mmy = (c0y + c1y) / 2

    left = _subdivide_quadratic(x0, y0, c0x, c0y, mmx, mmy, depth + 1, max_depth)
    right = _subdivide_quadratic(mmx, mmy, c1x, c1y, x1, y1, depth + 1, max_depth)
    return left + right[1:]


def _mat4_to_bytes(m: glm.mat4) -> bytes:
    """Convert glm.mat4 to bytes for shader uniform."""
    import struct
    values = []
    for col in range(4):
        for row in range(4):
            values.append(m[col][row])
    return struct.pack("16f", *values)
