"""Select tool — move shapes via bounding box handles (scale/rotate not yet implemented)."""
from __future__ import annotations

from imgui_bundle import imgui

from ui.swf_editor.tools import BaseTool


class SelectTool(BaseTool):
    name = "select"
    cursor = "arrow"

    def __init__(self, app):
        super().__init__(app)
        self._dragging = False
        self._drag_start_x = 0.0
        self._drag_start_y = 0.0

    def on_mouse_down(self, x: float, y: float, button: int) -> bool:
        if button != 0:
            return False

        cx, cy = self.app.camera.screen_to_canvas(x, y)

        # Hit test: find shape under cursor
        scene = self.app.scene
        entries = scene.get_visible_entries()

        # Simple AABB hit test
        hit_layer = -1
        hit_entry = -1
        for layer_idx in range(len(scene.layers) - 1, -1, -1):
            layer = scene.layers[layer_idx]
            if not layer.visible or layer.locked:
                continue
            kf = layer.keyframe_at(scene.current_frame)
            if not kf:
                continue
            for entry_idx, de in enumerate(kf.display_list):
                shape = scene.library_symbols.get(de.shape_id)
                if not shape:
                    continue
                bx, by, bw, bh = shape.bounds_px
                tx = de.transform.x + bx
                ty = de.transform.y + by
                tw = (bw - bx) * de.transform.scale_x
                th = (bh - by) * de.transform.scale_y
                if tx <= cx <= tx + tw and ty <= cy <= ty + th:
                    hit_layer = layer_idx
                    hit_entry = entry_idx
                    break
            if hit_layer >= 0:
                break

        if hit_layer >= 0:
            scene.selection = [(hit_layer, hit_entry)]
            self._dragging = True
            self._drag_start_x = cx
            self._drag_start_y = cy
            self.app.push_undo("Move")
        else:
            scene.selection = []

        return True

    def on_mouse_move(self, x: float, y: float) -> None:
        if not self._dragging:
            return

        cx, cy = self.app.camera.screen_to_canvas(x, y)
        dx = cx - self._drag_start_x
        dy = cy - self._drag_start_y

        scene = self.app.scene
        for layer_idx, entry_idx in scene.selection:
            layer = scene.layers[layer_idx]
            kf = layer.keyframe_at(scene.current_frame)
            if kf and entry_idx < len(kf.display_list):
                kf.display_list[entry_idx].transform.x += dx
                kf.display_list[entry_idx].transform.y += dy

        self._drag_start_x = cx
        self._drag_start_y = cy
        scene.dirty = True

    def on_mouse_up(self, x: float, y: float, button: int) -> None:
        self._dragging = False

    def draw_overlay(self, draw_list, camera) -> None:
        """Draw selection handles around selected shapes."""
        scene = self.app.scene
        for layer_idx, entry_idx in scene.selection:
            if layer_idx >= len(scene.layers):
                continue
            layer = scene.layers[layer_idx]
            kf = layer.keyframe_at(scene.current_frame)
            if not kf or entry_idx >= len(kf.display_list):
                continue
            de = kf.display_list[entry_idx]
            shape = scene.library_symbols.get(de.shape_id)
            if not shape:
                continue

            bx, by, bw, bh = shape.bounds_px
            # Convert bounds corners to screen space
            corners = [
                camera.canvas_to_screen(de.transform.x + bx, de.transform.y + by),
                camera.canvas_to_screen(de.transform.x + bw, de.transform.y + by),
                camera.canvas_to_screen(de.transform.x + bw, de.transform.y + bh),
                camera.canvas_to_screen(de.transform.x + bx, de.transform.y + bh),
            ]
            color = imgui.get_color_u32(imgui.ImVec4(0.3, 0.6, 1.0, 1.0))
            for i in range(4):
                p1 = corners[i]
                p2 = corners[(i + 1) % 4]
                draw_list.add_line(
                    imgui.ImVec2(p1[0], p1[1]),
                    imgui.ImVec2(p2[0], p2[1]),
                    color, 1.5,
                )
            # Corner handles
            for px, py in corners:
                draw_list.add_rect_filled(
                    imgui.ImVec2(px - 3, py - 3),
                    imgui.ImVec2(px + 3, py + 3),
                    color,
                )
