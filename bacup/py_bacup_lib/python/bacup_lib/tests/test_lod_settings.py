import json

import pytest

from bacup_lib.lod_settings import (
    PROFILE_CHOICES,
    available_profiles,
    cross_game_default_settings,
    load_profile_settings,
)


def test_unavailable_performance_profile_falls_back_to_high_quality(tmp_path):
    profile_dir = tmp_path / "bacup" / "scripts" / "lod_settings"
    profile_dir.mkdir(parents=True)
    profile_path = profile_dir / "high-quality.skyrimsefo4.json"
    profile_path.write_text(
        json.dumps({"global": {"worldspaces": ["Tamriel"]}}),
        encoding="utf-8",
    )

    settings = load_profile_settings(
        [tmp_path],
        profile="performance",
        lod_mode="hybrid-atlas",
        pair_id="skyrimse:fo4",
    )

    assert settings == {
        "global": {"worldspaces": ["Tamriel"]},
        "_pair_id": "skyrimse:fo4",
    }


def test_available_profiles_reports_shipped_profiles_per_pair():
    assert available_profiles("fo76:fo4") == (
        "native",
        "high-quality",
        "performance",
    )
    assert available_profiles("skyrimse:fo4") == ("high-quality",)
    assert available_profiles("fnvfo3:fo4") == ()


@pytest.mark.parametrize("profile", PROFILE_CHOICES)
def test_pair_without_shipped_profile_uses_engine_defaults(tmp_path, profile):
    settings = load_profile_settings(
        [tmp_path],
        profile=profile,
        lod_mode="generate",
        pair_id="fnvfo3:fo4",
    )

    assert settings == cross_game_default_settings()
    assert settings["objects"]["source"] == "records"
    assert settings["global"]["worldspaces"] == []
    assert settings["global"]["stride"] is None
    assert settings["global"]["bounds"] is None
    assert settings["global"]["generate_trees"] is True


def test_unknown_profile_is_still_rejected(tmp_path):
    with pytest.raises(ValueError, match="unknown LOD settings profile"):
        load_profile_settings(
            [tmp_path],
            profile="ultra",
            lod_mode="generate",
            pair_id="fnvfo3:fo4",
        )
