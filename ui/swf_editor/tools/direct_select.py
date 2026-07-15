"""Direct select tool — edit individual anchor points and bezier handles."""
from __future__ import annotations

import math

from imgui_bundle import imgui

from ui.swf_editor.tools import BaseTool
from creation_lib.swf.shapes import StraightEdge, CurvedEdge, StyleChange


class DirectSelectTool(BaseTool):
    name = "direct_select"
    cursor = "arrow"

    def __init__(self, app):
        super().__init__(app)
        self._selected_record_idx: int = -1
        self._dragging = False
        self._drag_start_x = 0.0
        self._drag_start_y = 0.0

    def on_mouse_down(self, x: float, y: float, button: int) -> bool:
        if button != 0:
            return False

        cx, cy = self.app.camera.screen_to_canvas(x, y)
        scene = self.app.scene

        # Must have a shape selected first
        if not scene.selection:
            return False

        layer_idx, entry_idx = scene.selection[0]
        layer = scene.layers[layer_idx]
        kf = layer.keyframe_at(scene.current_frame)
        if not kf or entry_idx >= len(kf.display_list):
            return False

        de = kf.display_list[entry_idx]
        shape = scene.library_symbols.get(de.shape_id)
        if not shape:
            return False

        # Walk shape records and find nearest anchor point
        best_idx = -1
        best_dist = float("inf")
        cur_x, cur_y = 0.0, 0.0
        threshold = 6.0 / self.app.camera.zoom

        for i, rec in enumerate(shape.records):
            if isinstance(rec, StyleChange) and rec.has_move:
                cur_x, cur_y = rec.move_x / 20.0, rec.move_y / 20.0
            elif isinstance(rec, StraightEdge):
                cur_x += rec.dx / 20.0
                cur_y += rec.dy / 20.0
            elif isinstance(rec, CurvedEdge):
                cur_x += (rec.cx + rec.ax) / 20.0
                cur_y += (rec.cy + rec.ay) / 20.0

            # Check distance to this point (transformed)
            px = de.transform.x + cur_x
            py = de.transform.y + cur_y
            dist = math.hypot(cx - px, cy - py)
            if dist < threshold and dist < best_dist:
                best_dist = dist
                best_idx = i

        if best_idx >= 0:
            self._selected_record_idx = best_idx
            self._dragging = True
            self._drag_start_x = cx
            self._drag_start_y = cy
            self.app.push_undo("Move Anchor")
        else:
            self._selected_record_idx = -1

        return True

    def on_mouse_move(self, x: float, y: float) -> None:
        if not self._dragging or self._selected_record_idx < 0:
            return

        cx, cy = self.app.camera.screen_to_canvas(x, y)
        dx = (cx - self._drag_start_x) * 20  # convert to twips
        dy = (cy - self._drag_start_y) * 20

        scene = self.app.scene
        if not scene.selection:
            return

        layer_idx, entry_idx = scene.selection[0]
        layer = scene.layers[layer_idx]
        kf = layer.keyframe_at(scene.current_frame)
        if not kf or entry_idx >= len(kf.display_list):
            return

        de = kf.display_list[entry_idx]
        shape = scene.library_symbols.get(de.shape_id)
        if not shape or self._selected_record_idx >= len(shape.records):
            return

        rec = shape.records[self._selected_record_idx]
        if isinstance(rec, StyleChange) and rec.has_move:
            rec.move_x = int(rec.move_x + dx)
            rec.move_y = int(rec.move_y + dy)
        elif isinstance(rec, StraightEdge):
            rec.dx += dx
            rec.dy += dy
        elif isinstance(rec, CurvedEdge):
            rec.ax += dx
            rec.ay += dy

        self._drag_start_x = cx
        self._drag_start_y = cy
        scene.dirty = True

    def on_mouse_up(self, x: float, y: float, button: int) -> None:
        self._dragging = False

    def draw_overlay(self, draw_list, camera) -> None:
        """Draw anchor points and handles for selected shape."""
        scene = self.app.scene
        if not scene.selection:
            return

        layer_idx, entry_idx = scene.selection[0]
        if layer_idx >= len(scene.layers):
            return
        layer = scene.layers[layer_idx]
        kf = layer.keyframe_at(scene.current_frame)
        if not kf or entry_idx >= len(kf.display_list):
            return

        de = kf.display_list[entry_idx]
        shape = scene.library_symbols.get(de.shape_id)
        if not shape:
            return

        point_color = imgui.get_color_u32(imgui.ImVec4(1.0, 1.0, 1.0, 1.0))
        selected_color = imgui.get_color_u32(imgui.ImVec4(0.3, 0.6, 1.0, 1.0))
        handle_color = imgui.get_color_u32(imgui.ImVec4(0.5, 0.5, 1.0, 0.6))

        cur_x, cur_y = 0.0, 0.0
        for i, rec in enumerate(shape.records):
            if isinstance(rec, StyleChange) and rec.has_move:
                cur_x, cur_y = rec.move_x / 20.0, rec.move_y / 20.0
            elif isinstance(rec, StraightEdge):
                cur_x += rec.dx / 20.0
                cur_y += rec.dy / 20.0
            elif isinstance(rec, CurvedEdge):
                # Draw control point handle
                ctrl_x = cur_x + rec.cx / 20.0
                ctrl_y = cur_y + rec.cy / 20.0
                sc = camera.canvas_to_screen(
                    de.transform.x + cur_x, de.transform.y + cur_y)
                sh = camera.canvas_to_screen(
                    de.transform.x + ctrl_x, de.transform.y + ctrl_y)
                draw_list.add_line(
                    imgui.ImVec2(sc[0], sc[1]),
                    imgui.ImVec2(sh[0], sh[1]),
                    handle_color, 1.0,
                )
                draw_list.add_circle_filled(
                    imgui.ImVec2(sh[0], sh[1]), 3, handle_color)

                cur_x = ctrl_x + rec.ax / 20.0
                cur_y = ctrl_y + rec.ay / 20.0
            else:
                continue

            # Draw anchor point
            sp = camera.canvas_to_screen(
                de.transform.x + cur_x, de.transform.y + cur_y)
            c = selected_color if i == self._selected_record_idx else point_color
            draw_list.add_circle_filled(imgui.ImVec2(sp[0], sp[1]), 4, c)
