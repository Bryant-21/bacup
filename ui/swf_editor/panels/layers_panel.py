"""Layer stack panel -- ordering, visibility, lock, opacity."""
from __future__ import annotations

from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.swf_editor.swf_editor_app import SwfEditorApp


class LayersPanel:
    def __init__(self, app: SwfEditorApp):
        self.app = app
        self._renaming_index: int = -1
        self._rename_buf: str = ""

    def draw(self) -> None:
        visible, _ = imgui.begin("Layers##swf")
        if not visible:
            imgui.end()
            return

        scene = self.app.scene

        # Add/Remove buttons
        if imgui.button("+##add_layer"):
            self.app.push_undo("Add Layer")
            scene.add_layer()
        imgui.same_line()
        if imgui.button("-##del_layer") and len(scene.layers) > 1:
            self.app.push_undo("Delete Layer")
            scene.remove_layer(scene.active_layer_index)

        imgui.separator()

        # Layer list (top to bottom = last to first in list)
        for i in range(len(scene.layers) - 1, -1, -1):
            layer = scene.layers[i]
            is_active = i == scene.active_layer_index

            # Visibility toggle
            changed, layer.visible = imgui.checkbox(f"##vis_{i}", layer.visible)

            imgui.same_line()

            # Lock toggle
            lock_label = "L" if layer.locked else "U"
            if imgui.small_button(f"{lock_label}##lock_{i}"):
                layer.locked = not layer.locked

            imgui.same_line()

            # Layer name (selectable)
            flags = imgui.SelectableFlags_.none
            clicked, _ = imgui.selectable(
                f"{layer.name}##layer_{i}",
                is_active,
                flags,
                imgui.ImVec2(0, 0),
            )
            if clicked:
                scene.active_layer_index = i

            # Opacity slider
            if is_active:
                changed, layer.opacity = imgui.slider_float(
                    f"Opacity##layer_op_{i}", layer.opacity, 0.0, 1.0
                )

            # Context menu
            if imgui.begin_popup_context_item(f"layer_ctx_{i}"):
                if imgui.menu_item("Duplicate")[0]:
                    self.app.push_undo("Duplicate Layer")
                    import copy
                    dup = copy.deepcopy(layer)
                    dup.name = f"{layer.name} copy"
                    scene.layers.insert(i, dup)
                if imgui.menu_item("Delete")[0] and len(scene.layers) > 1:
                    self.app.push_undo("Delete Layer")
                    scene.remove_layer(i)
                imgui.end_popup()

        imgui.end()
