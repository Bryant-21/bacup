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
    ],
)
def test_apply_asset_prefix_returns_unprefixed_known_root(
    source_id: str,
    raw_path: str,
    expected: str,
) -> None:
    result = apply_asset_prefix(raw_path, GAME_PROFILES[source_id])
    assert result.replace("\\", "/") == expected


def test_apply_asset_prefix_is_idempotent() -> None:
    profile = GAME_PROFILES["fnv"]
    once = apply_asset_prefix("Meshes/foo.nif", profile)
    twice = apply_asset_prefix(once, profile)
    assert twice == once


def test_apply_asset_prefix_preserves_unknown_root() -> None:
    profile = GAME_PROFILES["fnv"]
    assert apply_asset_prefix("interface/foo.swf", profile) == "interface/foo.swf"


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
    ],
)
def test_apply_asset_prefix_for_root_returns_unprefixed_asset_paths(
    raw_path: str,
    root: str,
    expected: str,
) -> None:
    profile = GAME_PROFILES["fnv"]
    assert apply_asset_prefix_for_root(raw_path, profile, root) == expected


@pytest.mark.parametrize("raw_path", ["Null", "null", "0ABC12:FalloutNV.esm", "interface/foo.swf"])
def test_apply_asset_prefix_for_root_preserves_non_asset_values(raw_path: str) -> None:
    profile = GAME_PROFILES["fnv"]
    assert apply_asset_prefix_for_root(raw_path, profile, "Meshes") == raw_path
