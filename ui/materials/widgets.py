"""Generic field rendering for the material editor.

Provides draw_field() which dispatches to the correct ImGui widget based on
the FieldDef.kind, handling version visibility and dependency checks.
"""

from __future__ import annotations

from imgui_bundle import imgui

from .field_registry import FieldDef

# Fixed label column width for two-column form layout
_LABEL_WIDTH = 210.0


def _browse_path(attr: str, current: str, kind: str) -> tuple[str, bool]:
    """Open a native file dialog for a path field. Returns (new_value, changed)."""
    try:
        from creation_lib.ui.widgets.pick_folder import pick_file
        from ui.shared.path_utils import to_game_relative_path

        if kind == "texture_path":
            filetypes = [("DDS Textures", "*.dds"), ("All files", "*.*")]
            file_type = "texture"
        else:  # material_path
            filetypes = [("Material files", "*.bgsm *.bgem"), ("All files", "*.*")]
            file_type = "material"

        filepath = pick_file(f"Browse {attr}", filetypes)

        if filepath:
            return to_game_relative_path(filepath, file_type), True
    except Exception:
        pass

    return current, False


def draw_field(field: FieldDef, value, version: int, fields_dict: dict) -> tuple[bool, object]:
    """Render a single field widget. Returns (changed, new_value).

    Returns (False, value) without drawing if the field is hidden by
    version or dependency rules.
    """
    # 1. Version visibility
    if field.version_visible and not field.version_visible(version):
        return False, value

    # 2. Dependency check
    if field.depends_on and not fields_dict.get(field.depends_on):
        return False, value

    # 3. Layout: label left, widget right
    imgui.text(field.name)
    if field.tooltip:
        if imgui.is_item_hovered():
            imgui.set_tooltip(field.tooltip)
    imgui.same_line(_LABEL_WIDTH)
    imgui.set_next_item_width(-1)

    # 4. Dispatch by kind
    kind = field.kind
    tag = f"##{field.attr}"

    if kind == "bool":
        changed, new_val = imgui.checkbox(tag, bool(value))
        return changed, new_val

    elif kind == "float":
        changed, new_val = imgui.slider_float(tag, float(value or 0.0), v_min=0.0, v_max=1.0)
        return changed, new_val

    elif kind == "int":
        changed, new_val = imgui.input_int(tag, int(value or 0))
        return changed, new_val

    elif kind == "color3":
        col = list(value) if value else [1.0, 1.0, 1.0]
        changed, col = imgui.color_edit3(tag, col)
        if changed:
            return True, (col[0], col[1], col[2])
        return False, value

    elif kind == "string":
        changed, new_val = imgui.input_text(tag, str(value or ""))
        return changed, new_val

    elif kind in ("texture_path", "material_path"):
        # Text input sized to leave room for browse button
        imgui.set_next_item_width(-35)
        changed, new_val = imgui.input_text(tag, str(value or ""))

        imgui.same_line()
        if imgui.small_button(f"...{tag}"):
            new_val, changed = _browse_path(field.attr, str(value or ""), kind)

        return changed, new_val

    elif kind == "dropdown":
        items = field.dropdown_items or []
        idx = int(value or 0)
        if idx < 0 or idx >= len(items):
            idx = 0
        changed, new_idx = imgui.combo(tag, idx, items)
        return changed, new_idx

    return False, value
