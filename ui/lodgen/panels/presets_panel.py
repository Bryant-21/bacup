"""Presets, Generate, progress, and log panel."""
from __future__ import annotations

import json

from imgui_bundle import imgui

from creation_lib.lod.default_settings import fo4_default_settings
from ui.lodgen.state import apply_preset, collect_preset


def draw(app) -> None:
    state = app.state

    # Preset buttons
    if imgui.button("Save Preset##lodgen_pre_save"):
        _save_preset(state)

    imgui.same_line()

    if imgui.button("Load Preset##lodgen_pre_load"):
        _load_preset(state)

    imgui.same_line()

    if imgui.button("Reset to Defaults##lodgen_pre_reset"):
        apply_preset(state, {"settings": fo4_default_settings()})

    imgui.separator()

    # Generate button (disabled while running)
    imgui.begin_disabled(state.running)
    if imgui.button("Generate##lodgen_pre_gen"):
        app.start_generate()
    imgui.end_disabled()

    # Progress bar
    imgui.progress_bar(state.progress_frac, (-1, 0), state.progress_msg or "")

    # Error message
    if state.error_message:
        imgui.push_style_color(imgui.Col_.text, (1.0, 0.4, 0.4, 1.0))
        imgui.text_wrapped(state.error_message)
        imgui.pop_style_color()

    imgui.separator()

    # Log pane (scrolling child)
    imgui.begin_child("##lodgen_pre_log", (0, 0), True)
    for line in state.log_lines[-200:]:
        imgui.text_unformatted(line)
    if state.running:
        imgui.set_scroll_here_y(1.0)
    imgui.end_child()


def _save_preset(state) -> None:
    try:
        from imgui_bundle import portable_file_dialogs as pfd  # type: ignore[import]
        dlg = pfd.save_file("Save LOD Preset", "preset.json", ["JSON Files", "*.json"])
        path = dlg.result()
        if path:
            with open(path, "w", encoding="utf-8") as f:
                json.dump(collect_preset(state), f, indent=2)
    except Exception:  # noqa: BLE001
        pass


def _load_preset(state) -> None:
    try:
        from imgui_bundle import portable_file_dialogs as pfd  # type: ignore[import]
        dlg = pfd.open_file("Load LOD Preset", ".", ["JSON Files", "*.json"])
        paths = dlg.result()
        if paths:
            with open(paths[0], encoding="utf-8") as f:
                preset = json.load(f)
            apply_preset(state, preset)
    except Exception:  # noqa: BLE001
        pass
