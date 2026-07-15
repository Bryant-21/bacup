"""Fill tool — fill bucket and eyedropper for shape colors."""
from __future__ import annotations

from imgui_bundle import imgui

from ui.swf_editor.tools import BaseTool
from creation_lib.swf.types import RGBA


class FillMode:
    BUCKET = "bucket"
    EYEDROPPER = "eyedropper"


class FillTool(BaseTool):
    name = "fill"
    cursor = "crosshair"

    def __init__(self, app):
        super().__init__(app)
        self.mode: str = FillMode.BUCKET
        self.active_color: RGBA = RGBA(255, 255, 255, 255)

    def on_mouse_down(self, x: float, y: float, button: int) -> bool:
        if button != 0:
            return False

        cx, cy = self.app.camera.screen_to_canvas(x, y)
        scene = self.app.scene

        # Find shape under cursor (same AABB hit test as select tool)
        hit_shape_id = None
        for layer_idx in range(len(scene.layers) - 1, -1, -1):
            layer = scene.layers[layer_idx]
            if not layer.visible or layer.locked:
                continue
            kf = layer.keyframe_at(scene.current_frame)
            if not kf:
                continue
            for de in kf.display_list:
                shape = scene.library_symbols.get(de.shape_id)
                if not shape:
                    continue
                bx, by, bw, bh = shape.bounds_px
                tx = de.transform.x + bx
                ty = de.transform.y + by
                tw = (bw - bx) * de.transform.scale_x
                th = (bh - by) * de.transform.scale_y
                if tx <= cx <= tx + tw and ty <= cy <= ty + th:
                    hit_shape_id = de.shape_id
                    break
            if hit_shape_id is not None:
                break

        if hit_shape_id is None:
            return True

        shape = scene.library_symbols[hit_shape_id]

        if self.mode == FillMode.EYEDROPPER:
            # Sample color from shape
            if shape.fill_styles:
                self.active_color = RGBA(
                    shape.fill_styles[0].color.r,
                    shape.fill_styles[0].color.g,
                    shape.fill_styles[0].color.b,
                    shape.fill_styles[0].color.a,
                )
        else:
            # Bucket: apply active color to shape
            if shape.fill_styles:
                self.app.push_undo("Fill Color")
                shape.fill_styles[0].color = RGBA(
                    self.active_color.r,
                    self.active_color.g,
                    self.active_color.b,
                    self.active_color.a,
                )
                scene.dirty = True

        return True

    def on_mouse_move(self, x: float, y: float) -> None:
        pass

    def on_mouse_up(self, x: float, y: float, button: int) -> None:
        pass

    def draw_overlay(self, draw_list, camera) -> None:
        """Draw active color swatch in corner."""
        r = self.active_color.r / 255.0
        g = self.active_color.g / 255.0
        b = self.active_color.b / 255.0
        a = self.active_color.a / 255.0
        color = imgui.get_color_u32(imgui.ImVec4(r, g, b, a))
        # Small swatch at bottom-left of viewport
        draw_list.add_rect_filled(
            imgui.ImVec2(10, draw_list.get_clip_rect_max().y - 30),
            imgui.ImVec2(30, draw_list.get_clip_rect_max().y - 10),
            color,
        )
