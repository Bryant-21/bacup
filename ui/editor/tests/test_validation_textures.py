"""Tests for cross-game texture validation with conversion suggestions."""
import pytest
from unittest.mock import MagicMock


def _make_mock_nif_with_texture_set(textures: list[str]):
    """Create mock NIF with BSTriShape referencing given texture paths."""
    nif = MagicMock()
    schema = MagicMock()
    nif.schema = schema

    shape = MagicMock()
    shape.type_name = "BSTriShape"
    shape.block_id = 0
    shape.get_field.side_effect = lambda name: 1 if name == "Shader Property" else None

    shader = MagicMock()
    shader.type_name = "BSLightingShaderProperty"
    shader.get_field.side_effect = lambda name: (
        2 if name == "Texture Set" else ""
    )

    tex_set = MagicMock()
    tex_set.type_name = "BSShaderTextureSet"
    # Pad to 8 slots
    padded = textures + [""] * (8 - len(textures))
    tex_set.get_field.return_value = padded

    blocks = [shape, shader, tex_set]
    nif.blocks = blocks
    schema.is_subtype_of.side_effect = lambda t, base: base in t
    nif.get_block.side_effect = lambda idx: blocks[idx] if 0 <= idx < len(blocks) else None

    return nif


def test_detects_wrong_game_texture_suffixes():
    """Validation should detect FO76 textures on a FO4 NIF using naming module."""
    from creation_lib.textures.naming import detect_texture_role
    from creation_lib.core.game_profiles import FO4_PROFILE, FO76_PROFILE

    # FO76-style _l suffix on what's supposed to be a FO4 NIF
    assert detect_texture_role("armor_l.dds", FO4_PROFILE) is None
    assert detect_texture_role("armor_l.dds", FO76_PROFILE) == "lighting"

    # FO4-style _s suffix on what's supposed to be FO76
    assert detect_texture_role("armor_s.dds", FO76_PROFILE) is None
    assert detect_texture_role("armor_s.dds", FO4_PROFILE) == "specular"


def test_check_cross_game_textures_finds_fo4_spec_on_fo76():
    """The enhanced check should detect _s.dds on a metallic-roughness NIF."""
    from ui.editor.panels.validation import ValidationPanel
    from creation_lib.core.game_profiles import FO76_PROFILE

    app = MagicMock()
    panel = ValidationPanel(app)
    nif = _make_mock_nif_with_texture_set([
        "armor_d.dds", "armor_n.dds", "", "", "", "", "", "armor_s.dds"
    ])

    panel._check_texture_naming(nif, FO76_PROFILE)

    assert len(panel._issues) == 1
    severity, block_id, msg = panel._issues[0]
    assert severity == "WARNING"
    assert "_s.dds" in msg


def test_check_cross_game_textures_clean_nif():
    """A NIF with correct texture naming should produce no warnings."""
    from ui.editor.panels.validation import ValidationPanel
    from creation_lib.core.game_profiles import FO4_PROFILE

    app = MagicMock()
    panel = ValidationPanel(app)
    nif = _make_mock_nif_with_texture_set([
        "armor_d.dds", "armor_n.dds", "", "", "", "", "", "armor_s.dds"
    ])

    panel._check_texture_naming(nif, FO4_PROFILE)

    # FO4 NIF with _s.dds is correct -- no warnings
    assert len(panel._issues) == 0
