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

        if self.app.document.mode == 'binary':
            self.app.stage_panel.draw_properties()
            imgui.end()
            return
        scene = self.app.scene
        selection = scene.selection
        imgui.begin_disabled(self.app.document.mode != "visual")
        imgui.push_item_width(-125)

        if not selection:
            self._draw_canvas_properties(scene)
        elif len(selection) == 1:
            self._draw_shape_properties(scene, selection[0])
        else:
            self._draw_multi_properties(scene, selection)

        imgui.pop_item_width()
        imgui.end_disabled()
        imgui.end()

    def _draw_canvas_properties(self, scene) -> None:
        imgui.text("Canvas")
        imgui.separator()

        imgui.text(f"Size: {scene.canvas_width} x {scene.canvas_height}")

        if self.app.document.mode != "visual":
            imgui.text(f"FPS: {scene.fps:g}")
            changed, val = False, scene.fps
        else:
            changed, val = imgui.slider_int("FPS##swf_fps", int(scene.fps), 1, 60)
        if changed:
            self.app.push_undo("Change FPS")
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
            self.app.push_undo("Canvas color")
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

        imgui.begin_disabled(layer.locked)
        for label, field, lower, upper in (("X", "x", -scene.canvas_width, scene.canvas_width * 2),
                                            ("Y", "y", -scene.canvas_height, scene.canvas_height * 2),
                                            ("Scale X", "scale_x", -10, 10), ("Scale Y", "scale_y", -10, 10),
                                            ("Rotation", "rotation", -360, 360)):
            changed, value = imgui.slider_float(f"{label}##swf", getattr(t, field), lower, upper)
            if changed:
                self.app.push_undo("Transform")
                setattr(t, field, value)
        changed, alpha = imgui.slider_int("Instance alpha##swf", entry.color_alpha, 0, 255)
        if changed:
            self.app.push_undo("Instance alpha")
            entry.color_alpha = alpha
        changed, name = imgui.input_text("Instance name##swf", entry.instance_name)
        if changed:
            self.app.push_undo("Instance name")
            entry.instance_name = name

        imgui.separator()
        imgui.text("Fill Color")
        shape = scene.library_symbols.get(entry.shape_id)
        if shape and shape.fill_styles:
            fs = shape.fill_styles[0]
            if fs.color:
                c = [fs.color.r / 255.0, fs.color.g / 255.0,
                     fs.color.b / 255.0, fs.color.a / 255.0]
                changed, c = imgui.color_edit4("##fill_color", c)
                if changed:
                    self._set_fill(shape, c)

        imgui.separator()
        imgui.text("Palette")
        for name, (r, g, b, a) in _PALETTE:
            if imgui.color_button(f"##{name}", imgui.ImVec4(r, g, b, a), 0, imgui.ImVec2(20, 20)):
                self._set_fill(shape, (r, g, b, a))
            imgui.same_line()
            imgui.text(name)
        imgui.end_disabled()

    def _set_fill(self, shape, color):
        from creation_lib.swf.types import RGBA
        if shape and shape.fill_styles:
            self.app.push_undo("Fill color")
            shape.fill_styles[0].color = RGBA(*(round(c * 255) for c in color))
            shape.fill_styles[0].fill_type = 0
            if self.app.renderer:
                self.app.renderer.invalidate_shape(shape.shape_id)
            if "fill" in self.app._tools:
                self.app._tools["fill"].active_color = shape.fill_styles[0].color

    def _draw_multi_properties(self, scene, selection) -> None:
        imgui.text(f"{len(selection)} shapes selected")
        imgui.separator()
        imgui.text("Shared properties shown as '---'")
