from __future__ import annotations

import pytest

from bacup_lib.source_pairs import (
    DEFAULT_PAIR_ID,
    FNV_MVP_EXCLUDE_SIGNATURES,
    FNV_REQUIRED_EXCLUDE_SIGNATURES,
    FNV_QUEST_SLICE_EXCLUDE_SIGNATURES,
    FO4_MVP_EXCLUDE_SIGNATURES,
    MVP_EXCLUDE_SIGNATURES_BY_PAIR,
    MVP_REQUIRED_PAIRS,
    SOURCE_PAIRS,
    SKYRIM_MVP_EXCLUDE_SIGNATURES,
    STARFIELD_MVP_EXCLUDE_SIGNATURES,
    get_pair,
    required_exclude_signatures,
)
from creation_lib.core.game_profiles import get_profile


def test_fnvfo3_default_admits_nonquest_records_and_gates_only_quest_runtime() -> None:
    assert required_exclude_signatures("fnvfo3:fo4") == FNV_REQUIRED_EXCLUDE_SIGNATURES
    assert required_exclude_signatures("skyrimse:fo4") == frozenset()
    assert required_exclude_signatures("fnvfo3:fo4") == {
        "DIAL", "INFO", "QUST", "SCEN"
    }
    assert {
        "ACRE", "AMMO", "BPTD", "CREA", "CSTY", "ENCH", "EYES", "HAIR",
        "HDPT", "LVLC", "LVLN", "PACK", "PERK", "PROJ", "RACE", "SPEL", "WEAP",
    }.isdisjoint(required_exclude_signatures("fnvfo3:fo4"))
    assert required_exclude_signatures(DEFAULT_PAIR_ID) == frozenset()


def test_all_supported_source_pairs_are_resolvable() -> None:
    assert DEFAULT_PAIR_ID == "fo76:fo4"
    assert set(SOURCE_PAIRS) == {
        "fo76:fo4",
        "fnvfo3:fo4",
        "skyrimse:fo4",
        "fo4:starfield",
        "starfield:fo4",
    }

    for pair_id in SOURCE_PAIRS:
        pair = get_pair(pair_id)
        assert pair.pair_id == pair_id
        assert pair.engine == get_profile(pair.source_game).engine


def test_unknown_pair_lists_available_pair_ids() -> None:
    with pytest.raises(KeyError) as exc_info:
        get_pair("nope")

    message = str(exc_info.value)
    assert "nope" in message
    for pair_id in sorted(SOURCE_PAIRS):
        assert pair_id in message


def test_world_only_mvp_exclusions_are_pair_specific() -> None:
    assert MVP_EXCLUDE_SIGNATURES_BY_PAIR == {
        "fnvfo3:fo4": FNV_MVP_EXCLUDE_SIGNATURES,
        "skyrimse:fo4": SKYRIM_MVP_EXCLUDE_SIGNATURES,
        "fo4:starfield": FO4_MVP_EXCLUDE_SIGNATURES,
        "starfield:fo4": STARFIELD_MVP_EXCLUDE_SIGNATURES,
    }
    assert {"QUST", "CREA", "RACE", "WEAP"} <= (
        FNV_MVP_EXCLUDE_SIGNATURES
    )
    assert {"NPC_", "ACHR", "ARMO", "ARMA"}.isdisjoint(
        FNV_MVP_EXCLUDE_SIGNATURES
    )
    assert "ACRE" in FNV_MVP_EXCLUDE_SIGNATURES
    assert "ACRE" in FNV_QUEST_SLICE_EXCLUDE_SIGNATURES
    assert "ACHR" not in FNV_QUEST_SLICE_EXCLUDE_SIGNATURES


def test_fo4_starfield_requires_world_only_fence():
    fence = required_exclude_signatures("fo4:starfield")
    assert fence == FO4_MVP_EXCLUDE_SIGNATURES
    for sig in ("NPC_", "RACE", "WEAP", "QUST", "DIAL", "INFO", "PACK", "PERK",
                "SCEN", "SMBN", "SMEN", "SMQN", "ARMO", "ARMA", "AMMO", "FURN",
                "TERM", "OMOD", "INNR", "SPEL", "ENCH", "CSTY", "LVLN", "PROJ",
                "BPTD", "CLFM", "EYES", "HDPT", "LSCR", "DLBR", "DLVW", "ACHR",
                "MOVT"):
        assert sig in fence, sig
    for sig in ("WRLD", "CELL", "REFR", "STAT", "SCOL", "MSTT", "ACTI", "LIGH",
                "TXST", "LTEX", "MATT", "WATR", "KYWD", "MUSC", "MUST", "SNDR",
                "REGN", "LGTM"):
        assert sig not in fence, sig


def test_starfield_mvp_fence_registered():
    assert "starfield:fo4" in MVP_REQUIRED_PAIRS
    assert required_exclude_signatures("starfield:fo4") == STARFIELD_MVP_EXCLUDE_SIGNATURES
    for sig in ("NPC_", "WEAP", "ARMO", "ARMA"):
        assert sig in STARFIELD_MVP_EXCLUDE_SIGNATURES
    assert "BIOM" in STARFIELD_MVP_EXCLUDE_SIGNATURES
    # OMOD/COBJ only modify or craft the fenced WEAP/ARMO/NPC_, so admitting
    # them ships orphans whose Starfield-format payloads crash FO4's loader in
    # BGSAttachParentArray::Load at startup.
    for sig in ("OMOD", "COBJ"):
        assert sig in STARFIELD_MVP_EXCLUDE_SIGNATURES
    for sig in ("FURN", "MSTT", "STAT", "CELL"):
        assert sig not in STARFIELD_MVP_EXCLUDE_SIGNATURES


def test_starfield_fence_signatures_exist_in_schema():
    from creation_lib.esp.schema import get_schema
    known = set(get_schema("starfield").records.keys())
    unknown = STARFIELD_MVP_EXCLUDE_SIGNATURES - known
    assert not unknown, f"fence lists sigs absent from the Starfield schema: {sorted(unknown)}"


def test_regen_paths_name_archives_after_the_output_plugin() -> None:
    """FO4 mounts "<PluginStem> - Main.ba2" / " - Textures.ba2" by PLUGIN name.

    Naming them after the mod folder instead ships archives the game never
    mounts: every converted asset goes missing, and the first SCOL to build
    dereferences a null model handle (BGSStaticCollection::Build3D) instead of
    logging a missing mesh. Only fnvfo3:fo4 has the two names differ, so this
    invariant is what keeps that pair honest.
    """
    from pathlib import Path

    from bacup_lib.regen_pipeline import RegenPaths

    def _paths(pair) -> RegenPaths:
        return RegenPaths(
            source_extracted_dir=Path("source"),
            source_data_dir=Path("source/Data"),
            target_data_dir=Path("target/Data"),
            target_ck_ini_path=Path("CreationKitCustom.ini"),
            target_custom_ini_path=Path("Fallout4Custom.ini"),
            target_game_ini_path=Path("Fallout4.ini"),
            output_root=Path("mods") / pair.output_mod_name,
            mod_name=pair.output_mod_name,
            archive_base_name=Path(pair.output_plugin_name).stem,
        )

    for pair_id, pair in SOURCE_PAIRS.items():
        expected = Path(pair.output_plugin_name).stem
        assert _paths(pair).archive_name == expected, pair_id

    assert _paths(get_pair("fnvfo3:fo4")).archive_name == "FalloutNV"
    assert get_pair("fnvfo3:fo4").output_mod_name == "FNV_FO3"
