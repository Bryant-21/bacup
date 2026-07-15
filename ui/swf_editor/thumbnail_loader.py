"""Background tessellation thread for shape gallery thumbnails.

The main GL thread cannot be used from a background thread (GL is not
thread-safe). This module handles ONLY CPU work:

  1. Deserialize shape_data JSON blob
  2. Reconstruct geometry (paths from records)
  3. Earcut tessellation -> flat vertex array

Results are posted to a queue consumed by the main thread, which then
does the GL upload (FBO render -> texture).
"""
from __future__ import annotations

import json
import logging
import queue
import threading
from typing import TYPE_CHECKING

_log = logging.getLogger(__name__)

# Thumbnail sizes available in the gallery
SIZES = (64, 96, 128)


class ThumbnailLoader:
    """Background thread: shape_data BLOB -> tessellated vertex array -> queue."""

    def __init__(self, result_queue: queue.Queue) -> None:
        """
        Args:
            result_queue: Queue where results are posted as
                          (shape_id: int, size: int, verts: list[float] | None, bounds: list)
        """
        self._result_queue = result_queue
        self._work_queue: queue.Queue = queue.Queue()
        self._thread = threading.Thread(target=self._run, daemon=True, name="ThumbnailLoader")
        self._thread.start()

    def request(self, shape_id: int, shape_data: bytes, bounds: list, size: int) -> None:
        """Queue a tessellation request. Non-blocking."""
        self._work_queue.put((shape_id, shape_data, bounds, size))

    def clear_pending(self) -> None:
        """Drain the work queue (call on search change to cancel stale requests)."""
        while not self._work_queue.empty():
            try:
                self._work_queue.get_nowait()
            except queue.Empty:
                break

    def shutdown(self) -> None:
        """Signal the thread to stop. Blocks until done."""
        self._work_queue.put(None)
        self._thread.join(timeout=2.0)

    def _run(self) -> None:
        while True:
            item = self._work_queue.get()
            if item is None:
                break
            shape_id, shape_data, bounds, size = item
            try:
                verts = _tessellate_from_blob(shape_data, bounds, size)
            except Exception as exc:
                _log.debug("Tessellation failed for shape %d: %s", shape_id, exc)
                verts = None
            self._result_queue.put((shape_id, size, verts, bounds))


def _tessellate_from_blob(shape_data: bytes, bounds: list, size: int) -> list[float]:
    """Deserialize shape_data BLOB and tessellate to vertex array.

    Returns flat list [x, y, r, g, b, a, ...] in pixel coordinates,
    scaled to fit within a (size x size) tile with 4px padding.

    Returns [] if shape_data is empty or unparseable.
    """
    if not shape_data:
        return []

    try:
        data = json.loads(shape_data.decode())
    except Exception:
        return []

    records = data.get("records", [])
    fill_styles = data.get("fill_styles", [])
    raw_bounds = data.get("bounds", [0.0, 0.0, 1.0, 1.0])  # px

    if not records or not fill_styles:
        return []

    # Parse fill colors
    def _hex_to_rgba(hex_str: str) -> tuple[float, float, float, float]:
        h = hex_str.lstrip("#")
        if len(h) == 6:
            h += "ff"
        r, g, b, a = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), int(h[6:8], 16)
        return r / 255.0, g / 255.0, b / 255.0, a / 255.0

    colors = [_hex_to_rgba(fs["color"]) for fs in fill_styles]

    # Walk records, collect rings grouped by fill color
    from creation_lib.swf.types import TWIPS_PER_PIXEL
    scale = 1.0 / TWIPS_PER_PIXEL

    all_vertices: list[float] = []
    rings: list[tuple[list[tuple[float, float]], tuple[float, float, float, float]]] = []
    current_path: list[tuple[float, float]] = []
    cx, cy = 0.0, 0.0
    current_color: tuple[float, float, float, float] = (1.0, 1.0, 1.0, 1.0)

    def _close_ring():
        nonlocal current_path
        if len(current_path) >= 3:
            rings.append((current_path, current_color))
        current_path = []

    for rec in records:
        rtype = rec.get("type")
        if rtype == "sc":
            if rec.get("move"):
                _close_ring()
                cx = rec.get("dx", 0) * scale
                cy = rec.get("dy", 0) * scale
                current_path.append((cx, cy))
            fill1 = rec.get("fill1")
            fill0 = rec.get("fill0")
            if fill1 and fill1 > 0 and fill1 - 1 < len(colors):
                current_color = colors[fill1 - 1]
            elif fill0 and fill0 > 0 and fill0 - 1 < len(colors):
                current_color = colors[fill0 - 1]
        elif rtype == "se":
            cx += rec.get("dx", 0) * scale
            cy += rec.get("dy", 0) * scale
            current_path.append((cx, cy))
        elif rtype == "ce":
            rx = rec.get("cx", 0) * scale
            ry = rec.get("cy", 0) * scale
            ax = rec.get("ax", 0) * scale
            ay = rec.get("ay", 0) * scale
            ex, ey = cx + rx + ax, cy + ry + ay
            mcx, mcy = cx + rx, cy + ry
            for i in range(1, 9):
                t = i / 8.0
                inv = 1.0 - t
                px = inv * inv * cx + 2 * inv * t * mcx + t * t * ex
                py = inv * inv * cy + 2 * inv * t * mcy + t * t * ey
                current_path.append((px, py))
            cx, cy = ex, ey
        elif rtype == "end":
            _close_ring()

    # Tessellate with even-odd fill: group rings by color, use earcut holes
    from earcut.earcut import earcut as _earcut

    color_groups: dict[tuple, list[list[tuple[float, float]]]] = {}
    for ring, color in rings:
        color_groups.setdefault(color, []).append(ring)

    for color, group_rings in color_groups.items():
        r, g, b, a = color
        _tessellate_even_odd_thumb(group_rings, r, g, b, a, all_vertices, _earcut)

    # Scale vertex positions to fit in (size x size) tile with 4px padding
    if not all_vertices:
        return []

    pad = 4.0
    inner = size - pad * 2
    xmin, ymin, xmax, ymax = raw_bounds
    w = max(xmax - xmin, 1.0)
    h = max(ymax - ymin, 1.0)
    s = min(inner / w, inner / h)

    result: list[float] = []
    for i in range(0, len(all_vertices), 6):
        x = (all_vertices[i] - xmin) * s + pad
        y = (all_vertices[i + 1] - ymin) * s + pad
        result.extend([x, y] + all_vertices[i + 2: i + 6])

    return result


def _tessellate_even_odd_thumb(
    group_rings: list[list[tuple[float, float]]],
    r: float, g: float, b: float, a: float,
    out: list[float],
    earcut_fn,
) -> None:
    """Tessellate rings with even-odd fill using pyclipper boolean XOR."""
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
        for ring in group_rings:
            if len(ring) < 3:
                continue
            flat = [v for pt in ring for v in pt]
            indices = earcut_fn(flat)
            for idx in indices:
                out.extend([ring[idx][0], ring[idx][1], r, g, b, a])
        return

    _walk_polytree(tree, r, g, b, a, out, earcut_fn)


def _walk_polytree(node, r, g, b, a, out, earcut_fn):
    """Recursively tessellate a pyclipper PolyTree."""
    import pyclipper

    for child in node.Childs:
        outer = pyclipper.scale_from_clipper(child.Contour)
        if len(outer) < 3:
            continue

        flat: list[float] = [v for pt in outer for v in pt]
        all_points: list[tuple[float, float]] = [(p[0], p[1]) for p in outer]
        hole_indices: list[int] = []

        for hole_node in child.Childs:
            hole = pyclipper.scale_from_clipper(hole_node.Contour)
            if len(hole) < 3:
                continue
            hole_indices.append(len(all_points))
            for pt in hole:
                all_points.append((pt[0], pt[1]))
                flat.extend([pt[0], pt[1]])

            if hole_node.Childs:
                _walk_polytree(hole_node, r, g, b, a, out, earcut_fn)

        indices = earcut_fn(flat, hole_indices if hole_indices else None)
        for idx in indices:
            out.extend([all_points[idx][0], all_points[idx][1], r, g, b, a])
