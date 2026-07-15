from __future__ import annotations

from bacup_lib.asset_paths import normalize_asset_source_path


def test_normalize_asset_source_path_strips_leading_data_prefix():
    assert (
        normalize_asset_source_path(
            "data/materials/Vehicles/Automotive/PickUpTruck03aA_Static.bgsm"
        )
        == "materials/Vehicles/Automotive/PickUpTruck03aA_Static.bgsm"
    )


def test_normalize_asset_source_path_strips_fo76_build_root():
    assert (
        normalize_asset_source_path(
            "C:/Projects/76/Build/PC/Materials/Landscape/Ground/TEMP_GroundTexture01Decal.BGSM"
        )
        == "Materials/Landscape/Ground/TEMP_GroundTexture01Decal.BGSM"
    )


def test_normalize_asset_source_path_strips_windows_fo76_build_root():
    assert (
        normalize_asset_source_path(
            "C:\\Projects\\76\\Build\\PC\\Materials\\Landscape\\Ground\\TEMP.BGSM"
        )
        == "Materials/Landscape/Ground/TEMP.BGSM"
    )


def test_normalize_asset_source_path_preserves_valid_relative_asset_path():
    assert normalize_asset_source_path("Meshes/Foo/Bar.nif") == "Meshes/Foo/Bar.nif"
