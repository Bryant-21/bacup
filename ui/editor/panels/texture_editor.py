"""Texture set editor — dedicated panel for BSShaderTextureSet editing.

Shows all 10 FO4 texture slots with text input, browse button, and clear button.
Auto-shows when a BSShaderTextureSet or a shape with one is selected.
"""

import logging

from imgui_bundle import imgui

_log = logging.getLogger("nif_editor.texture_editor")

# FO4 texture slot names (BSShaderTextureSet has up to 10 slots)
TEXTURE_SLOTS = [
    "Diffuse",
    "Normal",
    "Glow / Subsurface",
    "Height / Parallax",
    "Environment",
    "Environment Mask",
    "Multilayer",
    "Backlight / Specular",
    "Unused 8",
    "Unused 9",
]


class TextureEditorPanel:
    """Dedicated panel for editing BSShaderTextureSet texture slots."""

    def __init__(self, app):
        self.app = app
        self._visible = True
        self.window_name = "Texture Set Editor"
        self._selected_block_id = None

        if hasattr(app, 'selection_mgr'):
            app.selection_mgr.on_selection_changed(self._on_select)

    def _on_select(self, nif_id, block_id):
        self._selected_block_id = block_id

    def _find_texture_set(self):
        """Find the BSShaderTextureSet for the current selection.

        Returns (texture_set_block, via_shape) or (None, False).
        """
        nif = self.app.nif_file
        if not nif or self._selected_block_id is None:
            return None, False

        block = nif.get_block(self._selected_block_id)
        if not block:
            return None, False

        # Direct selection of texture set
        if block.type_name == "BSShaderTextureSet":
            return block, False

        # Shape → Shader Property → Texture Set
        if nif.schema.is_subtype_of(block.type_name, "BSTriShape"):
            sp_id = block.get_field("Shader Property")
            if isinstance(sp_id, dict):
                sp_id = sp_id.get("value", sp_id.get("Value", -1))
            if isinstance(sp_id, (int, float)) and int(sp_id) >= 0:
                sp = nif.get_block(int(sp_id))
                if sp:
                    return self._get_ts_from_shader(nif, sp), True
            return None, False

        # Shader property → Texture Set
        if block.type_name in ("BSLightingShaderProperty", "BSEffectShaderProperty"):
            return self._get_ts_from_shader(nif, block), True

        return None, False

    def _get_ts_from_shader(self, nif, shader_block):
        ts_id = shader_block.get_field("Texture Set")
        if isinstance(ts_id, dict):
            ts_id = ts_id.get("value", ts_id.get("Value", -1))
        if isinstance(ts_id, (int, float)) and int(ts_id) >= 0:
            ts = nif.get_block(int(ts_id))
            if ts and ts.type_name == "BSShaderTextureSet":
                return ts
        return None

    def draw(self):
        """Draw the texture editor panel."""
        if not self._visible:
            return

        ts_block, via_shape = self._find_texture_set()
        if not ts_block:
            return  # Don't show panel if no texture set

        expanded, opened = imgui.begin(self.window_name, True, imgui.WindowFlags_.no_focus_on_appearing)
        if not opened:
            self._visible = False
            imgui.end()
            return

        imgui.text_colored(
            imgui.ImVec4(0.9, 0.8, 0.5, 1.0),
            f"[{ts_block.block_id}] BSShaderTextureSet",
        )
        if via_shape:
            imgui.same_line()
            imgui.text_colored(imgui.ImVec4(0.5, 0.5, 0.5, 1.0), "(via shape)")
        imgui.separator()

        # Get textures — stored as individual fields "Textures[0]" through "Textures[9]"
        # or as a list field "Textures"
        textures = ts_block.get_field("Textures")
        if isinstance(textures, list):
            self._draw_texture_list(ts_block, textures)
        else:
            # Try individual fields
            for i in range(10):
                tex = ts_block.get_field(f"Textures[{i}]")
                if tex is not None:
                    self._draw_single_texture(ts_block, f"Textures[{i}]", i, tex)
                else:
                    # Try without brackets
                    tex = ts_block.get_field(f"Textures:{i}")
                    if tex is not None:
                        self._draw_single_texture(ts_block, f"Textures:{i}", i, tex)

        imgui.end()

    def _draw_texture_list(self, ts_block, textures: list):
        """Draw texture slots from a list field."""
        import copy
        changed_any = False
        old_textures = copy.deepcopy(textures)

        for i, tex in enumerate(textures):
            slot_name = TEXTURE_SLOTS[i] if i < len(TEXTURE_SLOTS) else f"Slot {i}"
            tex_str = _tex_to_string(tex)

            imgui.push_id(f"tex_{i}")
            imgui.text_colored(imgui.ImVec4(0.6, 0.8, 0.6, 1.0), f"[{i}] {slot_name}:")

            imgui.push_item_width(-80)
            changed, new_val = imgui.input_text(f"##tex_{i}", tex_str)
            imgui.pop_item_width()
            if changed:
                textures[i] = _string_to_tex(new_val)
                changed_any = True

            imgui.same_line()
            if imgui.small_button(f"...##browse_{i}"):
                result = self._browse_texture()
                if result is not None:
                    textures[i] = _string_to_tex(result)
                    changed_any = True

            imgui.same_line()
            if imgui.small_button(f"X##clear_{i}"):
                textures[i] = _string_to_tex("")
                changed_any = True

            imgui.pop_id()

        if changed_any:
            self._set_texture(
                ts_block,
                "Textures",
                old_textures,
                copy.deepcopy(textures),
            )

    def _draw_single_texture(self, ts_block, field_name: str, slot_idx: int, tex_value):
        """Draw a single texture slot from individual fields."""
        slot_name = TEXTURE_SLOTS[slot_idx] if slot_idx < len(TEXTURE_SLOTS) else f"Slot {slot_idx}"
        tex_str = _tex_to_string(tex_value)

        imgui.push_id(f"tex_{slot_idx}")
        imgui.text_colored(imgui.ImVec4(0.6, 0.8, 0.6, 1.0), f"[{slot_idx}] {slot_name}:")

        imgui.push_item_width(-80)
        changed, new_val = imgui.input_text(f"##tex_{slot_idx}", tex_str)
        imgui.pop_item_width()
        if changed:
            self._set_texture(ts_block, field_name, tex_value, _string_to_tex(new_val))

        imgui.same_line()
        if imgui.small_button(f"...##browse_{slot_idx}"):
            result = self._browse_texture()
            if result is not None:
                self._set_texture(ts_block, field_name, tex_value, _string_to_tex(result))

        imgui.same_line()
        if imgui.small_button(f"X##clear_{slot_idx}"):
            self._set_texture(ts_block, field_name, tex_value, _string_to_tex(""))

        imgui.pop_id()

    def _set_texture(self, ts_block, field_name: str, old_value: object, new_value: object):
        """Set a texture field with undo support."""
        if _texture_value_key(old_value) == _texture_value_key(new_value):
            return

        from creation_lib.nif.actions import SetFieldAction

        nif = self.app.nif_file
        cmd = SetFieldAction(
            block_id=ts_block.block_id,
            field_name=field_name,
            old_value=old_value,
            new_value=new_value,
        )
        cmd.execute(nif)
        self.app.undo_manager.push(self.app.registry.active_id, cmd)
        self.app._nif_dirty = True

    def _browse_texture(self) -> str | None:
        """Open file dialog for DDS textures, return game-relative path or None."""
        try:
            from creation_lib.ui.widgets.pick_folder import pick_file

            filepath = pick_file(
                "Select Texture",
                [("DDS Textures", "*.dds"), ("All files", "*.*")],
            )

            if filepath:
                # Convert to game-relative path
                filepath = filepath.replace("\\", "/")
                data_idx = filepath.lower().find("/data/")
                if data_idx >= 0:
                    return filepath[data_idx + 6:]
                return filepath
        except Exception:
            pass
        return None


def _tex_to_string(tex) -> str:
    """Convert a texture field value to a display string.

    NIF reader stores BSShaderTextureSet textures as:
      {'Length': N, 'Value': ['c','h','a','r','s']}
    or sometimes as plain strings.
    """
    if isinstance(tex, str):
        return tex
    if isinstance(tex, dict):
        val = tex.get("Value", tex.get("value", []))
        if isinstance(val, list):
            return "".join(str(c) for c in val)
        if isinstance(val, str):
            return val
        return str(val) if val else ""
    if isinstance(tex, list):
        return "".join(str(c) for c in tex)
    return str(tex) if tex else ""


def _string_to_tex(s: str) -> dict:
    """Convert a string back to the NIF texture field format.

    Returns {'Length': N, 'Value': [chars]} to match the NIF reader format.
    """
    chars = list(s) if s else []
    return {"Length": len(chars), "Value": chars}


def _texture_value_key(value: object):
    """Normalize texture field values for semantic comparisons."""
    if isinstance(value, list):
        return tuple(_texture_value_key(item) for item in value)
    return _tex_to_string(value)
