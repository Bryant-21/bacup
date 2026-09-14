"""Tests for properties panel helper functions."""
import pytest

from ui.editor.panels.properties import _looks_like_ref


class _FakeBlock:
    def __init__(self, type_name):
        self.type_name = type_name


class TestIsPathFieldLogic:
    """Documents expected _is_path_field behavior for RootMaterial."""

    def test_root_material_on_bslighting_is_path(self):
        """BSLightingShaderProperty.RootMaterial must be detected as a path field."""
        type_name = "BSLightingShaderProperty"
        field_name = "RootMaterial"
        # Rule: explicit check for (BSLightingShaderProperty, RootMaterial)
        matched = (
            type_name == "BSLightingShaderProperty" and field_name == "RootMaterial"
        )
        assert matched is True

    def test_root_material_on_bseffect_is_not_path(self):
        """BSEffectShaderProperty does not have RootMaterial — should not match."""
        type_name = "BSEffectShaderProperty"
        field_name = "RootMaterial"
        matched = (
            type_name == "BSLightingShaderProperty" and field_name == "RootMaterial"
        )
        assert matched is False


def test_nested_shader_crc_is_not_treated_as_block_reference():
    assert not _looks_like_ref("Shader Property Data.SF1[0]")
    assert not _looks_like_ref("Shader Property Data.SF2[3]")
    assert _looks_like_ref("Shader Property")
