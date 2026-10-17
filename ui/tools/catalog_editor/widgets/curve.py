"""Curve editor: a table of (x, y) points with add / remove / sort and a small plot.

Handles both shapes the catalogs use: ``[{"x": .., "y": ..}]`` and ``[[x, y], ..]``.
"""

from __future__ import annotations

from typing import Any

import numpy as np
from imgui_bundle import imgui


def is_object_points(node: dict, value: Any) -> bool:
    items = node.get("items", {})
    if isinstance(value, list) and value:
        return isinstance(value[0], dict)
    return isinstance(items, dict) and items.get("type") == "object"


def read_points(value: Any) -> list[tuple[float, float]]:
    points: list[tuple[float, float]] = []
    for point in value if isinstance(value, list) else []:
        if isinstance(point, dict):
            points.append((float(point.get("x", 0.0)), float(point.get("y", 0.0))))
        elif isinstance(point, (list, tuple)) and len(point) >= 2:
            points.append((float(point[0]), float(point[1])))
    return points


def write_points(points: list[tuple[float, float]], as_objects: bool) -> list:
    return [{"x": x, "y": y} for x, y in points] if as_objects else [[x, y] for x, y in points]


def draw(label: str, node: dict, value: Any, ctx, id_path: str) -> tuple[bool, Any]:
    as_objects = is_object_points(node, value)
    points = read_points(value)
    minimum = int(node.get("minItems", 0))
    changed = False
    if label:
        imgui.text(label)
    if node.get("description"):
        imgui.text_disabled(node["description"])
    if len(points) > 1:
        ys = np.array([y for _, y in points], dtype=np.float32)
        imgui.plot_lines("##curve", ys, graph_size=imgui.ImVec2(-1, max(60.0, imgui.get_content_region_avail().x * 0.18)))
    for index in range(len(points)):
        imgui.push_id(index)
        imgui.set_next_item_width(220)
        edited, xy = imgui.input_float2("x -> y", [points[index][0], points[index][1]], "%g")
        if edited:
            points[index] = (xy[0], xy[1])
            changed = True
        imgui.same_line()
        if imgui.small_button("x") and len(points) > minimum:
            del points[index]
            changed = True
            imgui.pop_id()
            break
        imgui.pop_id()
    if imgui.small_button("+ Point"):
        last = points[-1] if points else (-1.0, 0.0)
        points.append((last[0] + 1.0, last[1]))
        changed = True
    imgui.same_line()
    if imgui.small_button("Sort by x"):
        ordered = sorted(points, key=lambda p: p[0])
        changed = changed or ordered != points
        points = ordered
    return changed, (write_points(points, as_objects) if changed else value)
