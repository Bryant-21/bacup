"""Read-only, selectable JSON view of a record or the settings, with a Copy button."""

from __future__ import annotations

import json
from typing import Any

POPUP_ID = "View JSON##catalog_editor_json"


def json_text(value: Any) -> str:
    return json.dumps(value, indent=2)


class JsonViewer:
    def __init__(self) -> None:
        self._wants_open = False
        self._title = ""
        self._text = ""

    def show(self, title: str, value: Any) -> None:
        self._title, self._text, self._wants_open = title, json_text(value), True

    def draw(self) -> None:
        from imgui_bundle import imgui

        if self._wants_open:
            imgui.open_popup(POPUP_ID)
            self._wants_open = False
        viewport = imgui.get_main_viewport().size
        imgui.set_next_window_size(imgui.ImVec2(min(800, viewport.x * 0.9), min(600, viewport.y * 0.85)),
                                   imgui.Cond_.appearing)
        opened, _ = imgui.begin_popup_modal(POPUP_ID, None)
        if not opened:
            return
        imgui.text(self._title)
        imgui.input_text_multiline("##json", self._text, imgui.ImVec2(-1, -40), imgui.InputTextFlags_.read_only)
        if imgui.button("Copy"):
            imgui.set_clipboard_text(self._text)
        imgui.same_line()
        if imgui.button("Close"):
            imgui.close_current_popup()
        imgui.end_popup()
