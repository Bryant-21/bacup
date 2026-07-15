from __future__ import annotations

from dataclasses import FrozenInstanceError, replace

from imgui_bundle import imgui


_GAMES = ["fo4", "fo76", "skyrimse", "fnv", "fo3", "oblivion"]


def draw(app) -> None:
    state = app.state
    game_idx = _GAMES.index(state.game) if state.game in _GAMES else 0
    changed, new_idx = imgui.combo("Game##world_viewer_game", game_idx, _GAMES)
    if changed:
        state.game = _GAMES[new_idx]

    changed, worldspace = imgui.input_text("Worldspace##world_viewer_worldspace", state.worldspace)
    if changed:
        state.worldspace = worldspace

    changed, plugin_text = imgui.input_text(
        "Plugins##world_viewer_plugins",
        ";".join(state.plugin_paths),
    )
    if changed:
        state.plugin_paths = [part.strip() for part in plugin_text.split(";") if part.strip()]

    bounds = state.bounds
    changed, value = imgui.input_int("Min X##world_viewer_min_x", int(getattr(bounds, "min_x", -1)))
    if changed:
        _set_bound(app, "min_x", value)
    changed, value = imgui.input_int("Min Y##world_viewer_min_y", int(getattr(bounds, "min_y", -1)))
    if changed:
        _set_bound(app, "min_y", value)
    changed, value = imgui.input_int("Max X##world_viewer_max_x", int(getattr(bounds, "max_x", 1)))
    if changed:
        _set_bound(app, "max_x", value)
    changed, value = imgui.input_int("Max Y##world_viewer_max_y", int(getattr(bounds, "max_y", 1)))
    if changed:
        _set_bound(app, "max_y", value)

    if imgui.button("List Worldspaces##world_viewer_list_worldspaces"):
        app.load_worldspaces()
    imgui.same_line()
    if imgui.button("Load##world_viewer_load_scene"):
        app.load_scene()

    if state.worldspaces:
        labels = state.worldspaces
        idx = labels.index(state.worldspace) if state.worldspace in labels else 0
        changed, new_idx = imgui.combo("Known##world_viewer_known_worldspaces", idx, labels)
        if changed:
            state.worldspace = labels[new_idx]

    if state.error_message:
        imgui.text_wrapped(state.error_message)


def _set_bound(app, attr: str, value: int) -> None:
    bounds = app.state.bounds
    try:
        setattr(bounds, attr, value)
    except FrozenInstanceError:
        app.state.bounds = replace(bounds, **{attr: value})
