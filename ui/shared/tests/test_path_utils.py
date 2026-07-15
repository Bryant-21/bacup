"""Tests for ui/shared/path_utils.py"""
import pytest
from ui.shared.path_utils import to_game_relative_path


class TestToGameRelativePath:
    # --- Texture paths ---

    def test_texture_with_data_folder(self):
        result = to_game_relative_path(
            "X:/FO4/Data/Textures/Weapons/10mm/receiver_d.dds", "texture"
        )
        assert result == "Textures/Weapons/10mm/receiver_d.dds"

    def test_texture_mod_folder_no_data(self):
        """Mod folders may not have a Data/ segment."""
        result = to_game_relative_path(
            "X:/MyMod/Textures/Weapons/10mm/receiver_d.dds", "texture"
        )
        assert result == "Textures/Weapons/10mm/receiver_d.dds"

    def test_texture_backslashes_normalized(self):
        result = to_game_relative_path(
            "X:\\FO4\\Data\\Textures\\Weapons\\10mm\\d.dds", "texture"
        )
        assert result == "Textures/Weapons/10mm/d.dds"

    def test_texture_case_insensitive_textures_segment(self):
        """Segment matching is case-insensitive."""
        result = to_game_relative_path(
            "X:/FO4/Data/textures/Weapons/d.dds", "texture"
        )
        assert result == "textures/Weapons/d.dds"

    def test_texture_fallback_data_only(self):
        """If no Textures/ segment but Data/ is present, strip to after Data/."""
        result = to_game_relative_path(
            "X:/FO4/Data/SomeOtherFolder/d.dds", "texture"
        )
        assert result == "SomeOtherFolder/d.dds"

    def test_texture_no_known_segment_returns_filename(self):
        """Last resort: return filename only."""
        result = to_game_relative_path("X:/random/d.dds", "texture")
        assert result == "d.dds"

    # --- Material paths ---

    def test_material_with_data_materials(self):
        result = to_game_relative_path(
            "X:/FO4/Data/Materials/template/WeaponMetalTemplate_Wet.bgsm", "material"
        )
        assert result == "template/WeaponMetalTemplate_Wet.bgsm"

    def test_material_mod_folder_no_data(self):
        """Mod folders: strip from after Materials/."""
        result = to_game_relative_path(
            "X:/MyMod/Materials/template/WeaponMetalTemplate_Wet.bgsm", "material"
        )
        assert result == "template/WeaponMetalTemplate_Wet.bgsm"

    def test_material_backslashes_normalized(self):
        result = to_game_relative_path(
            "X:\\FO4\\Data\\Materials\\template\\Wet.bgsm", "material"
        )
        assert result == "template/Wet.bgsm"

    def test_material_fallback_data_only(self):
        """If no Materials/ segment but Data/ present, strip to after Data/."""
        result = to_game_relative_path(
            "X:/FO4/Data/template/Wet.bgsm", "material"
        )
        assert result == "template/Wet.bgsm"

    def test_material_no_known_segment_returns_filename(self):
        result = to_game_relative_path("X:/random/Wet.bgsm", "material")
        assert result == "Wet.bgsm"

    def test_material_case_insensitive_materials_segment(self):
        result = to_game_relative_path(
            "X:/FO4/Data/materials/template/Wet.bgsm", "material"
        )
        assert result == "template/Wet.bgsm"
