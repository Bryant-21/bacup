"""Tree LOD settings panel."""
from __future__ import annotations

from imgui_bundle import imgui

_BILLBOARD_SIZES = [512, 1024, 2048, 4096]
_BILLBOARD_LABELS = [str(s) for s in _BILLBOARD_SIZES]


def draw(app) -> None:
    state = app.state
    trees = state.settings.get("trees", {})

    changed, val = imgui.checkbox("Trees 3D##lodgen_tree_3d", bool(trees.get("trees_3d", True)))
    if changed:
        trees["trees_3d"] = val

    changed, val = imgui.checkbox("Generate Billboards##lodgen_tree_gb", bool(trees.get("generate_billboards", False)))
    if changed:
        trees["generate_billboards"] = val

    # Billboard atlas size combo
    current = int(trees.get("billboard_atlas_size", 2048))
    idx = _BILLBOARD_SIZES.index(current) if current in _BILLBOARD_SIZES else 2
    changed, new_idx = imgui.combo("Billboard Atlas Size##lodgen_tree_bas", idx, _BILLBOARD_LABELS)
    if changed:
        trees["billboard_atlas_size"] = _BILLBOARD_SIZES[new_idx]

    changed, val = imgui.slider_float(
        "Billboard Brightness##lodgen_tree_bb",
        float(trees.get("billboard_brightness", 1.0)),
        0.0,
        2.0,
    )
    if changed:
        trees["billboard_brightness"] = val
