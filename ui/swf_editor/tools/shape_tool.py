"""Shape tool — rectangle, ellipse, line primitives via click+drag."""
from __future__ import annotations

import math

from imgui_bundle import imgui

from ui.swf_editor.tools import BaseTool
from ui.swf_editor.swf_scene import EditorDisplayEntry, AffineTransform, Keyframe
from creation_lib.swf.shapes import ShapeDef, StraightEdge, CurvedEdge, StyleChange, EndShape
from creation_lib.swf.types import FillStyle, RGBA


class ShapeMode:
    RECTANGLE = "rectangle"
    ELLIPSE = "ellipse"
    LINE = "line"


class ShapeTool(BaseTool):
    name = "shape"
    cursor = "crosshair"

    def __init__(self, app):
        super().__init__(app)
        self.mode: str = ShapeMode.RECTANGLE
        self._drawing = False
        self._start_x = 0.0
        self._start_y = 0.0
        self._end_x = 0.0
        self._end_y = 0.0

    def on_mouse_down(self, x: float, y: float, button: int) -> bool:
        if button != 0:
            return False

        cx, cy = self.app.camera.screen_to_canvas(x, y)
        self._drawing = True
        self._start_x = cx
        self._start_y = cy
        self._end_x = cx
        self._end_y = cy
        return True

    def on_mouse_move(self, x: float, y: float) -> None:
        if not self._drawing:
            return
        cx, cy = self.app.camera.screen_to_canvas(x, y)

        # Shift constrains to square/circle
        io = imgui.get_io()
        if io.key_shift:
            dx = cx - self._start_x
            dy = cy - self._start_y
            side = max(abs(dx), abs(dy))
            cx = self._start_x + math.copysign(side, dx)
            cy = self._start_y + math.copysign(side, dy)

        self._end_x = cx
        self._end_y = cy

    def on_mouse_up(self, x: float, y: float, button: int) -> None:
        if not self._drawing:
            return
        self._drawing = False

        # Need minimum size
        dx = abs(self._end_x - self._start_x)
        dy = abs(self._end_y - self._start_y)
        if self.mode == ShapeMode.LINE:
            if dx < 1 and dy < 1:
                return
        elif dx < 2 or dy < 2:
            return

        self.app.push_undo(f"Draw {self.mode.title()}")

        x0 = min(self._start_x, self._end_x)
        y0 = min(self._start_y, self._end_y)
        x1 = max(self._start_x, self._end_x)
        y1 = max(self._start_y, self._end_y)
        w = x1 - x0
        h = y1 - y0

        if self.mode == ShapeMode.RECTANGLE:
            records = self._make_rect_records(w, h)
        elif self.mode == ShapeMode.ELLIPSE:
            records = self._make_ellipse_records(w, h)
        else:  # LINE
            records = self._make_line_records(
                self._start_x - x0, self._start_y - y0,
                self._end_x - x0, self._end_y - y0,
            )

        scene = self.app.scene
        shape_id = max(scene.library_symbols.keys(), default=0) + 1

        fill_styles = [FillStyle(fill_type=0, color=RGBA(255, 255, 255, 255))]
        shape = ShapeDef(
            shape_id=shape_id,
            bounds=(x0 * 20, y0 * 20, x1 * 20, y1 * 20),
            fill_styles=fill_styles,
            line_styles=[],
            records=records,
        )
        scene.library_symbols[shape_id] = shape

        layer = scene.layers[scene.active_layer_index]
        kf = layer.keyframe_at(scene.current_frame)
        if not kf:
            kf = Keyframe(frame=scene.current_frame)
            layer.keyframes.append(kf)
            layer.keyframes.sort(key=lambda k: k.frame)

        kf.display_list.append(EditorDisplayEntry(
            shape_id=shape_id,
            transform=AffineTransform(x=x0, y=y0),
        ))
        scene.dirty = True

    def _make_rect_records(self, w: float, h: float) -> list:
        """Rectangle as 4 straight edges in twips."""
        tw, th = w * 20, h * 20
        return [
            StyleChange(move_x=0, move_y=0, fill1=1),
            StraightEdge(dx=int(tw), dy=0),
            StraightEdge(dx=0, dy=int(th)),
            StraightEdge(dx=int(-tw), dy=0),
            StraightEdge(dx=0, dy=int(-th)),
            EndShape(),
        ]

    def _make_ellipse_records(self, w: float, h: float) -> list:
        """Ellipse approximated with 4 quadratic bezier arcs in twips."""
        # Kappa for quadratic bezier circle approximation
        rx, ry = w / 2.0 * 20, h / 2.0 * 20
        kx = rx * 0.5522847498
        ky = ry * 0.5522847498

        # Start at top center, go clockwise
        records = [
            StyleChange(move_x=int(rx), move_y=0, fill1=1),
            # Top-right quadrant
            CurvedEdge(cx=int(kx), cy=0, ax=int(rx - kx), ay=int(ry - ky)),
            CurvedEdge(cx=0, cy=int(ky), ax=int(-rx + kx), ay=int(ry - ky)),
            # Bottom-right to bottom-left
            CurvedEdge(cx=int(-kx), cy=0, ax=int(-(rx - kx)), ay=int(-(ry - ky))),
            CurvedEdge(cx=0, cy=int(-ky), ax=int(rx - kx), ay=int(-(ry - ky))),
            EndShape(),
        ]
        return records

    def _make_line_records(self, x0: float, y0: float, x1: float, y1: float) -> list:
        """Line as a single straight edge in twips."""
        return [
            StyleChange(move_x=int(x0 * 20), move_y=int(y0 * 20), fill1=1),
            StraightEdge(dx=int((x1 - x0) * 20), dy=int((y1 - y0) * 20)),
            EndShape(),
        ]

    def draw_overlay(self, draw_list, camera) -> None:
        """Draw shape preview while dragging."""
        if not self._drawing:
            return

        s0 = camera.canvas_to_screen(self._start_x, self._start_y)
        s1 = camera.canvas_to_screen(self._end_x, self._end_y)
        color = imgui.get_color_u32(imgui.ImVec4(0.3, 0.6, 1.0, 0.6))

        if self.mode == ShapeMode.LINE:
            draw_list.add_line(
                imgui.ImVec2(s0[0], s0[1]),
                imgui.ImVec2(s1[0], s1[1]),
                color, 1.5,
            )
        elif self.mode == ShapeMode.RECTANGLE:
            draw_list.add_rect(
                imgui.ImVec2(min(s0[0], s1[0]), min(s0[1], s1[1])),
                imgui.ImVec2(max(s0[0], s1[0]), max(s0[1], s1[1])),
                color, 0, 0, 1.5,
            )
        elif self.mode == ShapeMode.ELLIPSE:
            cx = (s0[0] + s1[0]) / 2
            cy = (s0[1] + s1[1]) / 2
            rx = abs(s1[0] - s0[0]) / 2
            ry = abs(s1[1] - s0[1]) / 2
            draw_list.add_ellipse(
                imgui.ImVec2(cx, cy), imgui.ImVec2(rx, ry), color, 0, 32, 1.5,
            )
