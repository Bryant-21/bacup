from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

SCRIPT_MEMBERS = {
    "Quests:Fishing:MQ01:BarrelTriggerScript": {"onactivate"},
    "Fishing:MQ01ChangeLocationQuestScript": {"onquestinit", "ontimer"},
    "Fishing:MQ01PlayerAliasScript": {
        "onaliasinit",
        "onaliasshutdown",
        "onitemadded",
    },
    "Fragments:Quests:QF_Fishing_MQ01_Casting_007ACB4C": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_0151_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_0800_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_0950_item_00",
        "fragment_stage_9000_item_00",
        "ontimer",
        "fishing_trystartbigfish",
    },
    "Fishing_BigFish_QuestScript": {"onquestinit", "onstageset"},
    "Fishing_BigFish_FishingScript": {
        "onaliasinit",
        "onaliasshutdown",
        "onitemadded",
    },
    "Fragments:Quests:QF_Fishing_BigFish_007B95E1": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0251_item_00",
        "fragment_stage_0252_item_00",
        "fragment_stage_0253_item_00",
        "fragment_stage_0254_item_00",
        "fragment_stage_0255_item_00",
        "fragment_stage_0256_item_00",
        "fragment_stage_0257_item_00",
        "fragment_stage_0270_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_9000_item_00",
    },
    "Quests:GHL00:QuestScript": {
        "onquestinit",
        "onstageset",
        "ontimer",
        "ghl00_updatepowerarmorobjective",
    },
    "Fragments:Quests:QF_GHL00_Quest_TransformPlay_007980FD": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_9900_item_00",
    },
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch(script_name)
    )


@pytest.mark.parametrize("script_name", SCRIPT_MEMBERS)
def test_recent_gap_patches_merge_once_without_declarations(script_name: str):
    patch = _patch(script_name)
    patch_members = _member_names(patch)
    assert set(patch_members) == SCRIPT_MEMBERS[script_name]
    assert len(patch_members) == len(SCRIPT_MEMBERS[script_name])
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )

    merged = _merged(script_name)
    merged_members = _member_names(merged)
    assert all(merged_members.count(member) == 1 for member in patch_members)
    assert _merge_script_method_patches(merged, patch) == merged


def test_casting_off_uses_story_manager_and_quest_specific_no_crafting_bridge():
    trigger = _patch("Quests:Fishing:MQ01:BarrelTriggerScript")
    casting = _patch("Fragments:Quests:QF_Fishing_MQ01_Casting_007ACB4C")
    counter = _patch("Fishing:MQ01PlayerAliasScript")

    assert "QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)" in trigger
    assert ".Start()" not in trigger
    assert "akBaseItem.HasKeyword(objectTypeFish)" in counter
    assert "owningQuest.SetStage(600)" in counter
    assert 'Game.GetFormFromFile(0x007CE310, "SeventySix.esm")' in casting
    assert "Fishing_ChumTrough_RewardThreshold.GetValue() as Int" in casting
    assert "playerRef.GetItemCount(fishBits) < requiredFishBits" in casting
    assert (
        "playerRef.AddItem(fishBits, requiredFishBits - "
        "playerRef.GetItemCount(fishBits))" in casting
    )
    assert "SetStage(800)" in casting
    assert "Fishing_BigFish_StartKeyword.SendStoryEventAndWait" in casting
    assert "If Fishing_TryStartBigFish()" in casting
    assert "StartTimer(5.0, 9000)" in casting
    assert "CompleteQuest()" in casting


def test_casting_off_tutorial_reaches_completion_after_satisfiable_feed():
    casting = _patch("Fragments:Quests:QF_Fishing_MQ01_Casting_007ACB4C")

    stage_700_start = casting.index("Function Fragment_Stage_0700_Item_00()")
    stage_800_start = casting.index("Function Fragment_Stage_0800_Item_00()")
    stage_700 = casting[stage_700_start:stage_800_start]
    assert (
        "requiredFishBits = Fishing_ChumTrough_RewardThreshold.GetValue() as Int"
        in stage_700
    )
    add_item = "playerRef.AddItem(fishBits, requiredFishBits - playerRef.GetItemCount(fishBits))"
    assert add_item in stage_700
    assert stage_700.index(add_item) < stage_700.index("SetStage(800)")

    stage_950_start = casting.index("Function Fragment_Stage_0950_Item_00()")
    stage_9000_start = casting.index("Function Fragment_Stage_9000_Item_00()")
    stage_950 = casting[stage_950_start:stage_9000_start]
    assert "Fishing_MQ01_FishingEnabledMessage.Show()" in stage_950
    assert "If !IsStageDone(9000)" in stage_950
    assert "SetStage(9000)" in stage_950


def test_big_fish_selects_one_local_region_and_counts_only_matching_fish():
    controller = _patch("Fishing_BigFish_QuestScript")
    counter = _patch("Fishing_BigFish_FishingScript")
    fragments = _patch("Fragments:Quests:QF_Fishing_BigFish_007B95E1")

    assert "Utility.RandomInt(0, RegionStages.Length - 1)" in controller
    assert "SetStage(RegionStages[regionIndex])" in controller
    assert "akBaseItem.HasKeyword(isAFish)" in counter
    assert "currentLocation.HasKeyword(regionHierarchyKeywords[chosenRegionInt])" in counter
    assert "BigFishQuestInstance.SetStage(allFishCaughtStage)" in counter
    for index, stage in enumerate(range(251, 258)):
        assert f"Fragment_Stage_0{stage}_Item_00" in fragments
        assert f"SetValue(chosenRegionAV, {index}.0)" in fragments
    assert "SetObjectiveDisplayed(40)" in fragments
    assert "CompleteQuest()" in fragments


def test_ghoul_transformation_patch_stops_at_the_native_boundary():
    controller = _patch("Quests:GHL00:QuestScript")
    fragments = _patch("Fragments:Quests:QF_GHL00_Quest_TransformPlay_007980FD")
    combined = controller + fragments

    assert "PlayerRef.IsInPowerArmor()" in controller
    assert "SetObjectiveDisplayed(Obj_ExitPA, inPowerArmor)" in controller
    assert "SetObjectiveDisplayed(Obj_EnterRadChamber, !inPowerArmor)" in controller
    assert "Fragment_Stage_0400_Item_00" in fragments
    assert "Fragment_Stage_9000_Item_00" not in fragments
    assert "GHL_GlowingOneCard" not in combined
    assert "GHL_RadiationPowerCard" not in combined
    assert "SetRace" not in combined
    assert "AddPerk" not in combined


@pytest.mark.parametrize("script_name", SCRIPT_MEMBERS)
def test_recent_gap_merged_sources_compile_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
