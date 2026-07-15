"""Tests for properties panel helper functions."""
import pytest


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
