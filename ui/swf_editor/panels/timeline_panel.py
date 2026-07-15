"""Frame timeline -- keyframe markers, playback controls, scrub bar."""
from __future__ import annotations

import time
from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.swf_editor.swf_editor_app import SwfEditorApp


class TimelinePanel:
    def __init__(self, app: SwfEditorApp):
        self.app = app
        self.onion_skin: bool = False
        self.onion_range: int = 2
        self._last_frame_time: float = 0.0

    def draw(self) -> None:
        visible, _ = imgui.begin("Timeline##swf")
        if not visible:
            imgui.end()
            return

        scene = self.app.scene
        total = max(scene.total_frames, 1)

        # Playback controls
        if imgui.button("<<##swf_tl"):
            scene.current_frame = 0
        imgui.same_line()

        play_label = "||" if scene.playing else ">"
        if imgui.button(f"{play_label}##swf_play"):
            scene.playing = not scene.playing
            self._last_frame_time = time.monotonic()
        imgui.same_line()

        if imgui.button(">>##swf_tl"):
            scene.current_frame = total - 1
        imgui.same_line()

        # Frame counter
        imgui.text(f"Frame {scene.current_frame + 1} / {total}")

        # Advance frame if playing
        if scene.playing:
            now = time.monotonic()
            if now - self._last_frame_time >= 1.0 / max(scene.fps, 1):
                scene.current_frame = (scene.current_frame + 1) % total
                self._last_frame_time = now

        # Scrub bar
        changed, frame = imgui.slider_int(
            "##swf_scrub", scene.current_frame, 0, max(total - 1, 0)
        )
        if changed:
            scene.current_frame = frame
            scene.playing = False

        imgui.separator()

        # Onion skin toggle
        _, self.onion_skin = imgui.checkbox("Onion Skin##swf", self.onion_skin)
        if self.onion_skin:
            imgui.same_line()
            _, self.onion_range = imgui.slider_int("Range##onion", self.onion_range, 1, 5)

        imgui.separator()

        # Frame grid visualization
        draw_list = imgui.get_window_draw_list()
        cursor = imgui.get_cursor_screen_pos()
        cell_w = 12.0
        cell_h = 20.0

        for layer_idx, layer in enumerate(scene.layers):
            y = cursor.y + layer_idx * (cell_h + 2)
            for f in range(min(total, 80)):  # Show up to 80 frames
                x = cursor.x + f * cell_w

                if f == scene.current_frame:
                    color = imgui.get_color_u32(imgui.ImVec4(0.3, 0.6, 1.0, 0.5))
                elif layer.keyframe_at(f):
                    color = imgui.get_color_u32(imgui.ImVec4(0.8, 0.8, 0.8, 1.0))
                else:
                    color = imgui.get_color_u32(imgui.ImVec4(0.3, 0.3, 0.3, 1.0))

                draw_list.add_rect_filled(
                    imgui.ImVec2(x, y),
                    imgui.ImVec2(x + cell_w - 1, y + cell_h - 1),
                    color,
                )

                # Keyframe dot
                kf = layer.keyframe_at(f)
                if kf and kf.frame == f:
                    dot_color = imgui.get_color_u32(imgui.ImVec4(1.0, 1.0, 1.0, 1.0))
                    draw_list.add_circle_filled(
                        imgui.ImVec2(x + cell_w / 2, y + cell_h / 2),
                        3.0, dot_color,
                    )

        # Reserve space for the grid
        imgui.dummy(imgui.ImVec2(80 * cell_w, len(scene.layers) * (cell_h + 2)))

        imgui.end()
