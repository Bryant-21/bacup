from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_QUEST_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "fragments"
    / "quests"
)


def _quest_script_name(base_name: str) -> str:
    return f"Fragments:Quests:{base_name}"


def _merged_production_quest_source(base_name: str) -> str:
    pex_path = DEPLOYED_QUEST_ROOT / f"{base_name.lower()}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(_quest_script_name(base_name))
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def _members_of(base_name: str) -> set[str]:
    patch = _script_patch_source(_quest_script_name(base_name))
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    return {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }


# --- QF_W05_DialogueRaidersCrater_00559532: reputation-dialogue orchestrator ---
# Stage 0 resets the 5 AwayValue AVs (per-AV None-guarded, none mandatory in the
# skeleton) only if the referenced Wastelanders-episode quest IsCompleted(). Stages
# 10/11/20/21 apply one of the 4 mandatory Rep_Mod_* globals to Reputation_AV_Crater,
# each guarded by its own documented tracking AV so a repeat dialogue trigger cannot
# re-apply reputation. Large/Medium/Small mapping is a disclosed flavor-tone
# inference (contracts/w3a-raiders.md Section B1 + Binding conditions resolution 2);
# every stage note was checked and contains no literal magnitude word.
CRATER_INTERIOR = "QF_W05_DialogueRaidersCrater_00559532"


def test_crater_interior_patch_members_match_stage_table():
    assert _members_of(CRATER_INTERIOR) == {
        "fragment_stage_0000_item_00",
        "fragment_stage_0010_item_00",
        "fragment_stage_0011_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0021_item_00",
    }


def test_crater_interior_stage_0000_resets_away_values_only_if_mq_complete():
    patch = _script_patch_source(_quest_script_name(CRATER_INTERIOR))
    assert patch is not None

    mq_guard = patch.find("W05_MQA_206P != None && W05_MQA_206P.IsCompleted()")
    assert mq_guard != -1
    for av_name in (
        "W05_MQR_RaRaAwayValue",
        "W05_MQR_MegAwayValue",
        "W05_MQR_LouAwayValue",
        "W05_MQR_JohnnyAwayValue",
        "W05_MQR_GailAwayValue",
    ):
        none_guard = patch.find(f"{av_name} != None")
        set_call = patch.find(f"player.SetValue({av_name}, 0.0)")
        assert none_guard != -1 and set_call != -1
        assert mq_guard < none_guard < set_call


def test_crater_interior_stages_guard_reputation_double_apply():
    patch = _script_patch_source(_quest_script_name(CRATER_INTERIOR))
    assert patch is not None

    # Meg: one combined tracking AV, two mutually-exclusive branches (1 vs 2).
    meg_10_guard = patch.find("player.GetValue(W05_PostMQ_ModRepTrackingAV_Meg) == 0.0")
    meg_10_mod = patch.find("player.ModValue(Reputation_AV_Crater, Rep_Mod_Add_Large.GetValue())")
    meg_10_set = patch.find("player.SetValue(W05_PostMQ_ModRepTrackingAV_Meg, 1.0)")
    meg_11_mod = patch.find("player.ModValue(Reputation_AV_Crater, Rep_Mod_Subtract_Small.GetValue())")
    meg_11_set = patch.find("player.SetValue(W05_PostMQ_ModRepTrackingAV_Meg, 2.0)")
    assert -1 not in (meg_10_guard, meg_10_mod, meg_10_set, meg_11_mod, meg_11_set)
    assert meg_10_guard < meg_10_mod < meg_10_set

    # Gail: two independent tracking AVs, one per stage.
    gail_parent_guard = patch.find("player.GetValue(W05_PostMQ_ModRepTrackingAV_Gail_Parent) == 0.0")
    gail_parent_mod = patch.find("player.ModValue(Reputation_AV_Crater, Rep_Mod_Add_Medium.GetValue())")
    gail_parent_set = patch.find("player.SetValue(W05_PostMQ_ModRepTrackingAV_Gail_Parent, 1.0)")
    gail_stupid_guard = patch.find("player.GetValue(W05_PostMQ_ModRepTrackingAV_Gail_Stupid) == 0.0")
    gail_stupid_mod = patch.find("player.ModValue(Reputation_AV_Crater, Rep_Mod_Subtract_Medium.GetValue())")
    gail_stupid_set = patch.find("player.SetValue(W05_PostMQ_ModRepTrackingAV_Gail_Stupid, 1.0)")
    assert -1 not in (
        gail_parent_guard, gail_parent_mod, gail_parent_set,
        gail_stupid_guard, gail_stupid_mod, gail_stupid_set,
    )
    assert gail_parent_guard < gail_parent_mod < gail_parent_set
    assert gail_stupid_guard < gail_stupid_mod < gail_stupid_set


def test_crater_interior_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_quest_source(CRATER_INTERIOR),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/Quests/{CRATER_INTERIOR}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


# --- QF_W05_DialogueRaidersCrater_0055E2BA: performance-scene package sync ---
# Stage 10 evaluates the 4 band-member aliases only ("Start Quest - Evaluate
# Packages - Band Members"), stage 100 the 7 audience aliases only ("Evaluate
# Packages - Audience"), stage 1000 all 12 aliases incl. Guard01 (the only stage
# note without a bucket qualifier -- "Scene over - Evaluate Packages" -- and the
# only stage that gives Guard01 a use, satisfying its mandatory property
# declaration), stage 9000 Stop() only.
CRATER_EXTERIOR_SCENE = "QF_W05_DialogueRaidersCrater_0055E2BA"

BAND_ALIASES = (
    "Alias_BandMember01",
    "Alias_BandMember02",
    "Alias_BandMember03",
    "Alias_BandMember04",
)
AUDIENCE_ALIASES = (
    "Alias_Audience01_Munch",
    "Alias_Audience02",
    "Alias_Audience03",
    "Alias_Audience04",
    "Alias_Audience05",
    "Alias_Audience06",
    "Alias_Audience07_Creed",
)


def test_crater_exterior_scene_patch_members_match_stage_table():
    assert _members_of(CRATER_EXTERIOR_SCENE) == {
        "evaluateifpresent",
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_9000_item_00",
    }


def test_crater_exterior_scene_evaluateifpresent_null_guards_before_evaluatepackage():
    patch = _script_patch_source(_quest_script_name(CRATER_EXTERIOR_SCENE))
    assert patch is not None

    guard_index = patch.find("target != None")
    call_index = patch.find("target.EvaluatePackage()")
    assert guard_index != -1 and call_index != -1
    assert guard_index < call_index


def test_crater_exterior_scene_stage_0010_evaluates_band_members_only():
    patch = _script_patch_source(_quest_script_name(CRATER_EXTERIOR_SCENE))
    assert patch is not None

    stage_start = patch.find("Function Fragment_Stage_0010_Item_00()")
    stage_end = patch.find("EndFunction", stage_start)
    body = patch[stage_start:stage_end]
    for alias in BAND_ALIASES:
        assert f"EvaluateIfPresent({alias})" in body
    for alias in AUDIENCE_ALIASES + ("Alias_Guard01",):
        assert f"EvaluateIfPresent({alias})" not in body


def test_crater_exterior_scene_stage_0100_evaluates_audience_only():
    patch = _script_patch_source(_quest_script_name(CRATER_EXTERIOR_SCENE))
    assert patch is not None

    stage_start = patch.find("Function Fragment_Stage_0100_Item_00()")
    stage_end = patch.find("EndFunction", stage_start)
    body = patch[stage_start:stage_end]
    for alias in AUDIENCE_ALIASES:
        assert f"EvaluateIfPresent({alias})" in body
    for alias in BAND_ALIASES + ("Alias_Guard01",):
        assert f"EvaluateIfPresent({alias})" not in body


def test_crater_exterior_scene_stage_1000_evaluates_everyone_including_guard():
    patch = _script_patch_source(_quest_script_name(CRATER_EXTERIOR_SCENE))
    assert patch is not None

    stage_start = patch.find("Function Fragment_Stage_1000_Item_00()")
    stage_end = patch.find("EndFunction", stage_start)
    body = patch[stage_start:stage_end]
    for alias in BAND_ALIASES + AUDIENCE_ALIASES + ("Alias_Guard01",):
        assert f"EvaluateIfPresent({alias})" in body


def test_crater_exterior_scene_stage_9000_stops_quest_only():
    patch = _script_patch_source(_quest_script_name(CRATER_EXTERIOR_SCENE))
    assert patch is not None

    stage_start = patch.find("Function Fragment_Stage_9000_Item_00()")
    stage_end = patch.find("EndFunction", stage_start)
    body = patch[stage_start:stage_end]
    assert "Stop()" in body
    assert "EvaluateIfPresent" not in body
    # Stop() appears nowhere else in the patch.
    assert patch.count("Stop()") == 1


def test_crater_exterior_scene_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_quest_source(CRATER_EXTERIOR_SCENE),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/Quests/{CRATER_EXTERIOR_SCENE}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


# --- QF_W05_DialogueRaidersCrater_005832EF: Aldridge death handler ---
# Single stage 500, triggered by the pre-existing DefaultAliasOnDeath script bound
# to the Aldridge alias (out of scope, untouched). The AV write is a literal match
# to that AV's own record Description ("If the player kills Aldridge... this gets
# set back to 0"), not an inference.
CRATER_OUTPOST = "QF_W05_DialogueRaidersCrater_005832EF"


def test_crater_outpost_patch_members_match_stage_table():
    assert _members_of(CRATER_OUTPOST) == {"fragment_stage_0500_item_00"}


def test_crater_outpost_stage_0500_resets_watchstation_av_guarded_by_player():
    patch = _script_patch_source(_quest_script_name(CRATER_OUTPOST))
    assert patch is not None

    guard_index = patch.find("player != None")
    set_index = patch.find("player.SetValue(W05_MQ_101P_A_ShortVersionCompleted, 0.0)")
    assert guard_index != -1 and set_index != -1
    assert guard_index < set_index


def test_crater_outpost_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_quest_source(CRATER_OUTPOST),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/Quests/{CRATER_OUTPOST}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_dialogueraiderscrater_patch_count_matches_shard_contract():
    assert len([CRATER_INTERIOR, CRATER_EXTERIOR_SCENE, CRATER_OUTPOST]) == 3
