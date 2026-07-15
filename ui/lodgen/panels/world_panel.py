"""World / worldspace picker panel."""
from __future__ import annotations

from imgui_bundle import imgui

_GAMES = ["fo4"]
_LEVEL_OPTIONS = [4, 8, 16, 32]


def draw(app) -> None:
    state = app.state
    g = state.settings.get("global", {})

    # Game selector
    game_idx = _GAMES.index(state.game) if state.game in _GAMES else 0
    changed, new_idx = imgui.combo("Game##lodgen_world_game", game_idx, _GAMES)
    if changed:
        state.game = _GAMES[new_idx]

    imgui.separator()

    # Worldspace input + list button
    changed, val = imgui.input_text("Worldspace##lodgen_world_ws", state.worldspace, 256)
    if changed:
        state.worldspace = val

    if imgui.button("List Worldspaces##lodgen_world_list"):
        app.load_worldspaces()

    if state.worldspaces:
        ws_list = list(state.worldspaces)
        try:
            ws_idx = ws_list.index(state.worldspace)
        except ValueError:
            ws_idx = 0
        changed, new_idx = imgui.combo("##lodgen_world_ws_combo", ws_idx, ws_list)
        if changed:
            state.worldspace = ws_list[new_idx]

    imgui.separator()

    # Output dir
    changed, val = imgui.input_text("Output Dir##lodgen_world_outdir", state.output_dir, 512)
    if changed:
        state.output_dir = val

    imgui.separator()

    # Specific chunk
    chunk = g.get("chunk")
    has_chunk = chunk is not None
    clicked, has_chunk = imgui.checkbox("Specific Chunk##lodgen_world_chunk", has_chunk)
    if clicked:
        if has_chunk:
            g["chunk"] = {"level": 4, "w": 0, "s": 0, "e": 0, "n": 0}
        else:
            g["chunk"] = None

    if has_chunk and isinstance(chunk, dict):
        level_idx = _LEVEL_OPTIONS.index(chunk.get("level", 4)) if chunk.get("level", 4) in _LEVEL_OPTIONS else 0
        level_labels = [str(x) for x in _LEVEL_OPTIONS]
        changed, new_idx = imgui.combo("Level##lodgen_world_chunk_level", level_idx, level_labels)
        if changed:
            chunk["level"] = _LEVEL_OPTIONS[new_idx]
        for axis in ("w", "s", "e", "n"):
            val_i = int(chunk.get(axis, 0))
            changed, new_val = imgui.slider_int(f"{axis.upper()}##lodgen_world_chunk_{axis}", val_i, -512, 512)
            if changed:
                chunk[axis] = new_val

    if state.error_message:
        imgui.text_wrapped(state.error_message)
