import pytest

from creation_lib.core.game_profiles import GAME_PROFILES
from bacup_lib.paths import (
    apply_asset_prefix,
    apply_asset_prefix_for_root,
)


@pytest.mark.parametrize(
    ("source_id", "raw_path", "expected"),
    [
        ("fnv", "Meshes/weapons/2hammer/club.nif", "Meshes/weapons/2hammer/club.nif"),
        (
            "fnv",
            "Textures/clutter/jukebox/jukebox_d.dds",
            "Textures/clutter/jukebox/jukebox_d.dds",
        ),
        (
            "fnv",
            "Materials/architecture/goodsprings/sign.bgsm",
            "Materials/architecture/goodsprings/sign.bgsm",
        ),
        (
            "fnv",
            "Sound/voice/falloutnv.esm/maleadult/foo.ogg",
            "Sound/voice/falloutnv.esm/maleadult/foo.ogg",
        ),
        ("fo76", "Meshes/foo/bar.nif", "Meshes/foo/bar.nif"),
        (
            "fnv",
            "Meshes/fnv/weapons/2hammer/club.nif",
            "Meshes/weapons/2hammer/club.nif",
        ),
        ("fnv", "interface/foo.swf", "interface/foo.swf"),
    ],
)
def test_apply_asset_prefix_returns_unprefixed_known_root(
    source_id: str,
    raw_path: str,
    expected: str,
) -> None:
    profile = GAME_PROFILES[source_id]
    result = apply_asset_prefix(raw_path, profile)
    assert result.replace("\\", "/") == expected
    assert apply_asset_prefix(result, profile) == result


@pytest.mark.parametrize(
    ("raw_path", "root", "expected"),
    [
        ("weapons/2hammer/club.nif", "Meshes", "Meshes/weapons/2hammer/club.nif"),
        (
            "clutter/jukebox/jukebox_d.dds",
            "Textures",
            "Textures/clutter/jukebox/jukebox_d.dds",
        ),
        ("FX/WPN/fire.wav", "Sound", "Sound/FX/WPN/fire.wav"),
        (
            "Textures/fnv/clutter/jukebox/jukebox_d.dds",
            "Textures",
            "Textures/clutter/jukebox/jukebox_d.dds",
        ),
        ("Null", "Meshes", "Null"),
        ("null", "Meshes", "null"),
        ("0ABC12:FalloutNV.esm", "Meshes", "0ABC12:FalloutNV.esm"),
        ("interface/foo.swf", "Meshes", "interface/foo.swf"),
    ],
)
def test_apply_asset_prefix_for_root_returns_unprefixed_asset_paths(
    raw_path: str,
    root: str,
    expected: str,
) -> None:
    profile = GAME_PROFILES["fnv"]
    assert apply_asset_prefix_for_root(raw_path, profile, root) == expected
