"""Pen tool — create bezier paths by clicking/dragging anchor points."""
from __future__ import annotations

import math

from imgui_bundle import imgui

from ui.swf_editor.tools import BaseTool
from ui.swf_editor.swf_scene import (
    EditorDisplayEntry, AffineTransform, Keyframe,
)
from creation_lib.swf.shapes import ShapeDef, CurvedEdge, StraightEdge, StyleChange, EndShape
from creation_lib.swf.types import FillStyle, RGBA


class PenTool(BaseTool):
    name = "pen"
    cursor = "crosshair"

    def __init__(self, app):
        super().__init__(app)
        # Each point: (x, y, handle_in_x, handle_in_y, handle_out_x, handle_out_y)
        self._points: list[tuple[float, float, float, float, float, float]] = []
        self._dragging_handle = False
        self._preview_x = 0.0
        self._preview_y = 0.0

    def on_mouse_down(self, x: float, y: float, button: int) -> bool:
        if button != 0:
            return False

        cx, cy = self.app.camera.screen_to_canvas(x, y)

        # Check if clicking near first point to close path
        if len(self._points) >= 3:
            fx, fy = self._points[0][0], self._points[0][1]
            if math.hypot(cx - fx, cy - fy) < 5.0 / self.app.camera.zoom:
                self._close_path()
                return True

        # Add new corner point (handles at same position = corner)
        self._points.append((cx, cy, cx, cy, cx, cy))
        self._dragging_handle = True
        return True

    def on_mouse_move(self, x: float, y: float) -> None:
        cx, cy = self.app.camera.screen_to_canvas(x, y)
        self._preview_x = cx
        self._preview_y = cy

        if self._dragging_handle and self._points:
            # Adjust handles of last point symmetrically
            px, py = self._points[-1][0], self._points[-1][1]
            dx, dy = cx - px, cy - py
            self._points[-1] = (px, py, px - dx, py - dy, cx, cy)

    def on_mouse_up(self, x: float, y: float, button: int) -> None:
        self._dragging_handle = False

    def on_key(self, key: int, down: bool) -> bool:
        if down and key == imgui.Key.escape:
            self._points.clear()
            return True
        return False

    def _close_path(self) -> None:
        """Convert accumulated points to a shape and add to active layer."""
        if len(self._points) < 3:
            self._points.clear()
            return

        scene = self.app.scene
        self.app.push_undo("Pen Path")

        # Build shape records from points
        records = []
        # Initial style change: set fill and move to first point
        first = self._points[0]
        records.append(StyleChange(
            move_x=int(first[0] * 20), move_y=int(first[1] * 20),
            fill1=1,
        ))

        for i in range(len(self._points)):
            p0 = self._points[i]
            p1 = self._points[(i + 1) % len(self._points)]

            # Check if this segment is a straight line (handles at anchor positions)
            is_line = (
                abs(p0[4] - p0[0]) < 0.01 and abs(p0[5] - p0[1]) < 0.01
                and abs(p1[2] - p1[0]) < 0.01 and abs(p1[3] - p1[1]) < 0.01
            )

            if is_line:
                dx = (p1[0] - p0[0]) * 20
                dy = (p1[1] - p0[1]) * 20
                records.append(StraightEdge(dx=int(dx), dy=int(dy)))
            else:
                # Cubic bezier: use handle_out of p0 and handle_in of p1
                # Convert cubic to quadratic approximation (midpoint method)
                # For SWF we need quadratic control point
                cx_ctrl = (p0[4] * 3 + p1[2] * 3 - p0[0] - p1[0]) / 4.0
                cy_ctrl = (p0[5] * 3 + p1[3] * 3 - p0[1] - p1[1]) / 4.0
                cdx = (cx_ctrl - p0[0]) * 20
                cdy = (cy_ctrl - p0[1]) * 20
                adx = (p1[0] - cx_ctrl) * 20
                ady = (p1[1] - cy_ctrl) * 20
                records.append(CurvedEdge(
                    cx=cdx, cy=cdy,
                    ax=adx, ay=ady,
                ))

        records.append(EndShape())

        # Create ShapeDef
        shape_id = max(scene.library_symbols.keys(), default=0) + 1
        shape = ShapeDef(
            shape_id=shape_id,
            bounds=(
                min(p[0] for p in self._points) * 20,
                min(p[1] for p in self._points) * 20,
                max(p[0] for p in self._points) * 20,
                max(p[1] for p in self._points) * 20,
            ),
            fill_styles=[FillStyle(fill_type=0, color=RGBA(255, 255, 255, 255))],
            line_styles=[],
            records=records,
        )
        scene.library_symbols[shape_id] = shape

        # Add to active layer's current keyframe
        layer = scene.layers[scene.active_layer_index]
        kf = layer.keyframe_at(scene.current_frame)
        if not kf:
            kf = Keyframe(frame=scene.current_frame)
            layer.keyframes.append(kf)
            layer.keyframes.sort(key=lambda k: k.frame)

        kf.display_list.append(EditorDisplayEntry(shape_id=shape_id))
        scene.dirty = True
        self._points.clear()

    def draw_overlay(self, draw_list, camera) -> None:
        """Draw in-progress path and preview."""
        if not self._points:
            return

        color = imgui.get_color_u32(imgui.ImVec4(0.3, 0.6, 1.0, 1.0))
        handle_color = imgui.get_color_u32(imgui.ImVec4(0.3, 0.6, 1.0, 0.5))

        # Draw existing segments
        for i in range(len(self._points) - 1):
            p0 = self._points[i]
            p1 = self._points[i + 1]
            s0 = camera.canvas_to_screen(p0[0], p0[1])
            s1 = camera.canvas_to_screen(p1[0], p1[1])
            draw_list.add_line(
                imgui.ImVec2(s0[0], s0[1]),
                imgui.ImVec2(s1[0], s1[1]),
                color, 1.5,
            )

        # Preview line to cursor
        last = self._points[-1]
        sl = camera.canvas_to_screen(last[0], last[1])
        sp = camera.canvas_to_screen(self._preview_x, self._preview_y)
        preview_color = imgui.get_color_u32(imgui.ImVec4(0.3, 0.6, 1.0, 0.4))
        draw_list.add_line(
            imgui.ImVec2(sl[0], sl[1]),
            imgui.ImVec2(sp[0], sp[1]),
            preview_color, 1.0,
        )

        # Draw anchor points
        for p in self._points:
            sp = camera.canvas_to_screen(p[0], p[1])
            draw_list.add_circle_filled(imgui.ImVec2(sp[0], sp[1]), 4, color)

            # Draw handles if they differ from anchor
            if abs(p[4] - p[0]) > 0.01 or abs(p[5] - p[1]) > 0.01:
                sh = camera.canvas_to_screen(p[4], p[5])
                draw_list.add_line(
                    imgui.ImVec2(sp[0], sp[1]),
                    imgui.ImVec2(sh[0], sh[1]),
                    handle_color, 1.0,
                )
                draw_list.add_circle_filled(imgui.ImVec2(sh[0], sh[1]), 3, handle_color)
