from __future__ import annotations

import pytest

from bacup_lib.asset_paths import normalize_asset_source_path


@pytest.mark.parametrize(
    "source,expected",
    [
        (
            "data/materials/Vehicles/Automotive/PickUpTruck03aA_Static.bgsm",
            "materials/Vehicles/Automotive/PickUpTruck03aA_Static.bgsm",
        ),
        (
            "C:/Projects/76/Build/PC/Materials/Landscape/Ground/TEMP_GroundTexture01Decal.BGSM",
            "Materials/Landscape/Ground/TEMP_GroundTexture01Decal.BGSM",
        ),
        (
            "C:\\Projects\\76\\Build\\PC\\Materials\\Landscape\\Ground\\TEMP.BGSM",
            "Materials/Landscape/Ground/TEMP.BGSM",
        ),
        ("Meshes/Foo/Bar.nif", "Meshes/Foo/Bar.nif"),
        (
            "DLC05/Effects/ DLC05MZRmGenerator01_d.NIF",
            "DLC05/Effects/DLC05MZRmGenerator01_d.NIF",
        ),
    ],
)
def test_normalize_asset_source_path(source: str, expected: str):
    assert normalize_asset_source_path(source) == expected
