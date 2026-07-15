"""Red crosshair overlay at viewport center for scope alignment."""
from __future__ import annotations

from imgui_bundle import imgui


# Crosshair appearance
_LINE_HALF = 20  # pixels from center
_LINE_COLOR = 0xFF0000FF  # ABGR: red, full alpha
_CIRCLE_RADIUS = 4
_LINE_THICKNESS = 2.0


def draw_crosshair(viewport_pos: imgui.ImVec2, viewport_size: imgui.ImVec2):
    """Draw a red crosshair at the center of the viewport."""
    draw_list = imgui.get_window_draw_list()

    cx = viewport_pos.x + viewport_size.x * 0.5
    cy = viewport_pos.y + viewport_size.y * 0.5

    draw_list.add_line(
        imgui.ImVec2(cx - _LINE_HALF, cy),
        imgui.ImVec2(cx + _LINE_HALF, cy),
        _LINE_COLOR, _LINE_THICKNESS,
    )
    draw_list.add_line(
        imgui.ImVec2(cx, cy - _LINE_HALF),
        imgui.ImVec2(cx, cy + _LINE_HALF),
        _LINE_COLOR, _LINE_THICKNESS,
    )
    draw_list.add_circle(
        imgui.ImVec2(cx, cy), _CIRCLE_RADIUS,
        _LINE_COLOR, 12, _LINE_THICKNESS,
    )
