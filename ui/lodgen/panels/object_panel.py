"""Object LOD settings panel."""
from __future__ import annotations

from imgui_bundle import imgui

from ui.lodgen.panels import _format_combo, _slider_float_setting, _slider_int_setting

_ATLAS_SIZES = [1024, 2048, 4096, 8192]
_ATLAS_LABELS = [str(s) for s in _ATLAS_SIZES]


def draw(app) -> None:
    state = app.state
    obj = state.settings.get("objects", {})

    changed, val = imgui.checkbox("Build Atlas##lodgen_obj_ba", bool(obj.get("build_atlas", True)))
    if changed:
        obj["build_atlas"] = val

    # Atlas size combo
    current_atlas = int(obj.get("atlas_size", 4096))
    idx = _ATLAS_SIZES.index(current_atlas) if current_atlas in _ATLAS_SIZES else 2
    changed, new_idx = imgui.combo("Atlas Size##lodgen_obj_as", idx, _ATLAS_LABELS)
    if changed:
        obj["atlas_size"] = _ATLAS_SIZES[new_idx]

    changed, val = imgui.checkbox(
        "Native Mip Flooding##lodgen_obj_mip_flood",
        bool(obj.get("atlas_mip_flooding", False)),
    )
    if changed:
        obj["atlas_mip_flooding"] = val

    _slider_float_setting("UV Range##lodgen_obj_uvr", obj, "uv_range", 1.0, 2.0)

    obj["diffuse_format"] = _format_combo("Diffuse Format##lodgen_obj_df", obj.get("diffuse_format", "Bc2"))
    obj["normal_format"] = _format_combo("Normal Format##lodgen_obj_nf", obj.get("normal_format", "Bc1"))
    obj["specular_format"] = _format_combo("Specular Format##lodgen_obj_sf", obj.get("specular_format", "Bc5"))

    _slider_int_setting("Max Tile Size##lodgen_obj_mts", obj, "max_tile_size", 1, 4096)
    _slider_int_setting("Alpha Threshold##lodgen_obj_at", obj, "alpha_threshold", 0, 255)

    for key, label in [
        ("use_alpha_threshold", "Use Alpha Threshold##lodgen_obj_uat"),
        ("use_backlight", "Use Backlight##lodgen_obj_ubl"),
        ("no_vertex_colors", "No Vertex Colors##lodgen_obj_nvc"),
        ("no_tangents", "No Tangents##lodgen_obj_nt"),
        ("remove_unseen_faces", "Remove Unseen Faces##lodgen_obj_ruf"),
    ]:
        changed, val = imgui.checkbox(label, bool(obj.get(key, False)))
        if changed:
            obj[key] = val
