from __future__ import annotations

import pytest

from bacup_lib.regen_pipeline import FO76_PLUGINS, OUTPUT_MOD_NAME
from bacup_lib.source_pairs import (
    DEFAULT_PAIR_ID,
    SOURCE_PAIRS,
    get_pair,
)
from creation_lib.core.game_profiles import get_profile


def test_all_supported_source_pairs_are_resolvable() -> None:
    assert DEFAULT_PAIR_ID == "fo76:fo4"
    assert set(SOURCE_PAIRS) == {"fo76:fo4", "fnvfo3:fo4", "skyrimse:fo4"}

    for pair_id in SOURCE_PAIRS:
        pair = get_pair(pair_id)
        assert pair.pair_id == pair_id
        assert pair.engine == get_profile(pair.source_game).engine


def test_default_pair_preserves_fo76_driver_constants() -> None:
    pair = get_pair(DEFAULT_PAIR_ID)

    assert pair.source_game == "fo76"
    assert pair.target_game == "fo4"
    assert pair.source_plugins == tuple(FO76_PLUGINS)
    assert pair.output_mod_name == OUTPUT_MOD_NAME
    assert pair.source_extracted_env == get_profile("fo76").env_var_name
    assert pair.source_data_env == "FO76_DATA_DIR"
    assert pair.source_dir_env == "FO76_DIR"
    assert pair.merge is None
    assert pair.optional_source_plugins == ()
    assert pair.output_plugin_name == "SeventySix.esm"


def test_merged_pair_lineages_include_official_plugins() -> None:
    fnvfo3 = get_pair("fnvfo3:fo4")
    assert fnvfo3.source_plugins == (
        "FalloutNV.esm",
        "DeadMoney.esm",
        "HonestHearts.esm",
        "OldWorldBlues.esm",
        "LonesomeRoad.esm",
        "GunRunnersArsenal.esm",
    )
    assert fnvfo3.optional_source_plugins == (
        "CaravanPack.esm",
        "ClassicPack.esm",
        "MercenaryPack.esm",
        "TribalPack.esm",
    )
    assert fnvfo3.merge is not None
    assert fnvfo3.merge.grafted_plugins == (
        "Fallout3.esm",
        "Anchorage.esm",
        "ThePitt.esm",
        "BrokenSteel.esm",
        "PointLookout.esm",
        "Zeta.esm",
    )
    assert fnvfo3.merge.grafted_data_env == "FO3_DATA_DIR"
    assert fnvfo3.merge.grafted_dir_env == "FO3_DIR"
    assert fnvfo3.output_plugin_name == "FNV_FO3_Merged.esm"

    skyrim = get_pair("skyrimse:fo4")
    assert skyrim.source_plugins == (
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    )
    assert skyrim.merge is not None
    assert skyrim.merge.grafted_game == "skyrimse"
    assert skyrim.merge.grafted_plugins == ()
    assert skyrim.output_plugin_name == "Skyrim_Merged.esm"


def test_unknown_pair_lists_available_pair_ids() -> None:
    with pytest.raises(KeyError) as exc_info:
        get_pair("nope")

    message = str(exc_info.value)
    assert "nope" in message
    for pair_id in sorted(SOURCE_PAIRS):
        assert pair_id in message
