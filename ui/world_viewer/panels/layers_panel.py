from __future__ import annotations

from dataclasses import FrozenInstanceError, replace

from imgui_bundle import imgui


_LAYERS = (
    ("Terrain", "include_terrain"),
    ("Statics", "include_statics"),
    ("Static Collections", "include_static_collections"),
    ("Markers", "include_markers"),
    ("Water", "include_water"),
    ("Lights", "include_lights"),
    ("Foliage", "include_foliage"),
    ("Disabled Refs", "include_disabled_refs"),
)


def draw(app) -> None:
    settings = app.state.settings
    for label, attr in _LAYERS:
        current = bool(getattr(settings, attr, False))
        changed, value = imgui.checkbox(f"{label}##world_viewer_{attr}", current)
        if changed:
            _set_layer(app, attr, value)


def _set_layer(app, attr: str, value: bool) -> None:
    settings = app.state.settings
    try:
        setattr(settings, attr, value)
    except FrozenInstanceError:
        app.state.settings = replace(settings, **{attr: value})
