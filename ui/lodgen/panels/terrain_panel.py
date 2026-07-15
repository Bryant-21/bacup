"""Terrain LOD settings panel."""
from __future__ import annotations

from imgui_bundle import imgui

from ui.lodgen.panels import _format_combo, _slider_float_setting, _slider_int_setting

_LEVEL_LABELS = ["LOD4", "LOD8", "LOD16", "LOD32"]
_SIZE_OPTIONS = [256, 512, 1024, 2048, 4096]
_OPTIMIZE_OPTIONS = ["Off", "On", "Depth"]


def draw(app) -> None:
    state = app.state
    t = state.settings.get("terrain", {})

    # Globals
    imgui.text("Globals")
    _slider_int_setting("Skirts##lodgen_terrain_skirts", t, "skirts", 0, 512)

    for key, label in [
        ("protect_cell_borders", "Protect Cell Borders##lodgen_terrain_pcb"),
        ("hide_quads", "Hide Quads##lodgen_terrain_hq"),
        ("underside", "Underside##lodgen_terrain_under"),
        ("heightmaps", "Heightmaps##lodgen_terrain_hm"),
        ("bake_normals", "Bake Normals##lodgen_terrain_bn"),
        ("bake_specular", "Bake Specular##lodgen_terrain_bs"),
    ]:
        changed, val = imgui.checkbox(label, bool(t.get(key, False)))
        if changed:
            t[key] = val

    _slider_float_setting("Brightness##lodgen_terrain_bright", t, "brightness", -1.0, 1.0)
    _slider_float_setting("Contrast##lodgen_terrain_contrast", t, "contrast", 0.0, 2.0)
    _slider_float_setting("Vertex Color Intensity##lodgen_terrain_vci", t, "vertex_color_intensity", 0.0, 2.0)

    gamma = t.get("gamma", [1.0, 1.0, 1.0])
    if isinstance(gamma, list) and len(gamma) == 3:
        for i, ch in enumerate(("R", "G", "B")):
            changed, val = imgui.slider_float(f"Gamma {ch}##lodgen_terrain_gamma_{i}", float(gamma[i]), 0.0, 2.0)
            if changed:
                gamma[i] = val
        t["gamma"] = gamma

    imgui.separator()

    # Per-level tabs
    opened, _ = imgui.begin_tab_bar("##lodgen_terrain_levels")
    if opened:
        levels = t.get("levels", [])
        for i, label in enumerate(_LEVEL_LABELS):
            if i < len(levels):
                level = levels[i]
                tab_opened, _ = imgui.begin_tab_item(f"{label}##lodgen_terrain_lvl_{i}")
                if tab_opened:
                    _draw_level(i, level)
                    imgui.end_tab_item()
        imgui.end_tab_bar()


def _draw_level(idx: int, level: dict) -> None:
    ns = f"##lodgen_terrain_l{idx}"
    changed, val = imgui.slider_float(f"Quality{ns}_q", float(level.get("quality", 10.0)), 1.0, 50.0)
    if changed:
        level["quality"] = val

    changed, val = imgui.slider_int(f"Max Vertices{ns}_mv", int(level.get("max_vertices", 32767)), 0, 65535)
    if changed:
        level["max_vertices"] = val

    # Optimize Unseen
    opt = level.get("optimize_unseen", "Off")
    if isinstance(opt, dict):
        opt_label = "Depth"
        opt_depth = float(next(iter(opt.values()), 2.0))
    else:
        opt_label = str(opt)
        opt_depth = 2.0
    opt_idx = _OPTIMIZE_OPTIONS.index(opt_label) if opt_label in _OPTIMIZE_OPTIONS else 0
    changed, new_idx = imgui.combo(f"Optimize Unseen{ns}_ou", opt_idx, _OPTIMIZE_OPTIONS)
    if changed:
        opt_label = _OPTIMIZE_OPTIONS[new_idx]
    if opt_label == "Depth":
        changed_d, opt_depth = imgui.slider_float(f"Depth{ns}_oud", opt_depth, 0.0, 10.0)
        level["optimize_unseen"] = {"Depth": opt_depth}
    else:
        level["optimize_unseen"] = opt_label

    # Diffuse
    _size_combo(f"Diffuse Size{ns}_ds", level, "diffuse_size")
    level["diffuse_format"] = _format_combo(f"Diffuse Format{ns}_df", level.get("diffuse_format", "Bc1"))
    changed, val = imgui.checkbox(f"Diffuse MipMap{ns}_dm", bool(level.get("diffuse_mipmap", True)))
    if changed:
        level["diffuse_mipmap"] = val

    # Normal
    _size_combo(f"Normal Size{ns}_ns", level, "normal_size")
    level["normal_format"] = _format_combo(f"Normal Format{ns}_nf", level.get("normal_format", "Bc1"))
    changed, val = imgui.checkbox(f"Normal MipMap{ns}_nm", bool(level.get("normal_mipmap", True)))
    if changed:
        level["normal_mipmap"] = val
    changed, val = imgui.slider_float(f"Normal Rise{ns}_nr", float(level.get("normal_rise", 1.0)), 0.0, 4.0)
    if changed:
        level["normal_rise"] = val


_SIZE_LABELS = [str(s) for s in [256, 512, 1024, 2048, 4096]]
_SIZE_VALUES = [256, 512, 1024, 2048, 4096]


def _size_combo(label: str, d: dict, key: str) -> None:
    current = int(d.get(key, 256))
    idx = _SIZE_VALUES.index(current) if current in _SIZE_VALUES else 0
    changed, new_idx = imgui.combo(label, idx, _SIZE_LABELS)
    if changed:
        d[key] = _SIZE_VALUES[new_idx]
