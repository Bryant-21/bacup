"""Context-sensitive property editor for selected shapes."""
from __future__ import annotations

from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.swf_editor.swf_editor_app import SwfEditorApp


# Pipboy color palette
_PALETTE = [
    ("White 100%", (1.0, 1.0, 1.0, 1.0)),
    ("White 60%", (1.0, 1.0, 1.0, 0.6)),
    ("White 40%", (1.0, 1.0, 1.0, 0.4)),
    ("White 25%", (1.0, 1.0, 1.0, 0.25)),
    ("Black", (0.0, 0.0, 0.0, 1.0)),
    ("BG Gray", (0.2, 0.2, 0.2, 1.0)),
]


class PropertiesPanel:
    def __init__(self, app: SwfEditorApp):
        self.app = app

    def draw(self) -> None:
        visible, _ = imgui.begin("Properties##swf")
        if not visible:
            imgui.end()
            return

        scene = self.app.scene
        selection = scene.selection

        if not selection:
            self._draw_canvas_properties(scene)
        elif len(selection) == 1:
            self._draw_shape_properties(scene, selection[0])
        else:
            self._draw_multi_properties(scene, selection)

        imgui.end()

    def _draw_canvas_properties(self, scene) -> None:
        imgui.text("Canvas")
        imgui.separator()

        imgui.text(f"Size: {scene.canvas_width} x {scene.canvas_height}")

        changed, val = imgui.slider_int("FPS##swf_fps", scene.fps, 1, 60)
        if changed:
            scene.fps = val
            scene.dirty = True

        imgui.text(f"Frames: {scene.total_frames}")
        imgui.text(f"Layers: {len(scene.layers)}")
        imgui.text(f"Shapes: {len(scene.library_symbols)}")

        imgui.separator()
        imgui.text("BG Color")
        bg = [scene.background.r / 255.0, scene.background.g / 255.0,
              scene.background.b / 255.0]
        changed, bg = imgui.color_edit3("##bg_color", bg)
        if changed:
            from creation_lib.swf.types import RGBA
            scene.background = RGBA(int(bg[0] * 255), int(bg[1] * 255), int(bg[2] * 255), 255)
            scene.dirty = True

    def _draw_shape_properties(self, scene, sel_pair) -> None:
        layer_idx, entry_idx = sel_pair
        if layer_idx >= len(scene.layers):
            return
        layer = scene.layers[layer_idx]
        kf = layer.keyframe_at(scene.current_frame)
        if not kf or entry_idx >= len(kf.display_list):
            return

        entry = kf.display_list[entry_idx]
        t = entry.transform

        imgui.text("Transform")
        imgui.separator()

        changed_x, t.x = imgui.slider_float("X##swf_x", t.x, -200.0, 750.0)
        changed_y, t.y = imgui.slider_float("Y##swf_y", t.y, -200.0, 600.0)
        changed_sx, t.scale_x = imgui.slider_float("Scale X##swf_sx", t.scale_x, 0.01, 10.0)
        changed_sy, t.scale_y = imgui.slider_float("Scale Y##swf_sy", t.scale_y, 0.01, 10.0)
        changed_r, t.rotation = imgui.slider_float("Rotation##swf_rot", t.rotation, -360.0, 360.0)

        if any([changed_x, changed_y, changed_sx, changed_sy, changed_r]):
            scene.dirty = True

        imgui.separator()
        imgui.text("Fill Color")
        shape = scene.library_symbols.get(entry.shape_id)
        if shape and shape.fill_styles:
            fs = shape.fill_styles[0]
            if fs.color:
                c = [fs.color.r / 255.0, fs.color.g / 255.0,
                     fs.color.b / 255.0, fs.color.a / 255.0]
                changed, c = imgui.color_edit4("##fill_color", c)

        imgui.separator()
        imgui.text("Palette")
        for name, (r, g, b, a) in _PALETTE:
            if imgui.color_button(f"##{name}", imgui.ImVec4(r, g, b, a), 0, imgui.ImVec2(20, 20)):
                pass  # TODO: apply this palette color to the selected shape's fill (not wired up yet)
            imgui.same_line()
            imgui.text(name)

    def _draw_multi_properties(self, scene, selection) -> None:
        imgui.text(f"{len(selection)} shapes selected")
        imgui.separator()
        imgui.text("Shared properties shown as '---'")
