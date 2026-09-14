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
QUEST_FRAGMENT = "Fragments:Quests:QF_TW010_0023C7F3"

QUEST_MEMBERS = {
    "scene.onend",
    "fragment_stage_0040_item_00",
    "fragment_stage_0050_item_00",
    "fragment_stage_0055_item_00",
    "fragment_stage_0060_item_00",
    "fragment_stage_0065_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0150_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0410_item_00",
    "fragment_stage_0420_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_0710_item_00",
    "fragment_stage_0720_item_00",
    "fragment_stage_1000_item_00",
}

PATCH_MEMBERS = {
    "TW010_MeatGrillScript": {"onactivate"},
    "TW010_VegeGrillScript": {"onactivate"},
    QUEST_FRAGMENT: QUEST_MEMBERS,
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_tw010_patch_is_member_only_and_complete(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_tw010_patch_merges_once_and_native_compiles(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member in members:
        assert names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_tw010_quest_start_and_retained_scenes_advance_the_stage_chain():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    quest_start = _member_body(patch, "fragment_stage_0100_item_00")
    scene_end = _member_body(patch, "scene.onend")

    assert 'RegisterForRemoteEvent(TW010_Intro, "OnEnd")' in quest_start
    assert 'RegisterForRemoteEvent(TW010_TurnInMeat, "OnEnd")' in quest_start
    assert 'RegisterForRemoteEvent(TW010_TurnInVeges, "OnEnd")' in quest_start
    assert "SetStage(100)" not in patch
    assert "akSender == TW010_Intro" in scene_end
    assert "TW010_Intro.IsActionComplete(1)" in scene_end
    assert "SetStage(150)" in scene_end
    assert "akSender == TW010_TurnInMeat" in scene_end
    assert "TW010_TurnInMeat.IsActionComplete(2)" in scene_end
    assert "SetStage(500)" in scene_end
    assert "akSender == TW010_TurnInVeges" in scene_end
    assert "TW010_TurnInVeges.IsActionComplete(2)" in scene_end
    assert "SetStage(1000)" in scene_end


def test_tw010_inventory_prerequisites_recheck_after_dialogue_transitions():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    meat_hunt = _member_body(patch, "fragment_stage_0150_item_00")
    side_dishes = _member_body(patch, "fragment_stage_0500_item_00")

    assert "playerRef.GetValue(Perception) >= PerceptionThreshhold" in meat_hunt
    assert "SetStage(40)" in meat_hunt
    assert "DefaultAliasInventoryManagementA" in meat_hunt
    assert "DefaultAliasInventoryManagementB" in meat_hunt
    assert "DefaultAliasInventoryManagementF" in meat_hunt
    assert meat_hunt.count(".EvaluateInventoryState()") == 3

    assert "DefaultAliasInventoryManagementC" in side_dishes
    assert "DefaultAliasInventoryManagementD" in side_dishes
    assert "DefaultAliasInventoryManagementE" in side_dishes
    assert side_dishes.count(".EvaluateInventoryState()") == 3


def test_tw010_grills_require_and_consume_the_bound_mandatory_ingredients():
    meat = _script_patch_source("TW010_MeatGrillScript")
    vegetables = _script_patch_source("TW010_VegeGrillScript")
    assert meat is not None
    assert vegetables is not None

    assert "akActionRef != playerRef" in meat
    assert "owningQuest.IsStageDone(PrereqStageMeat)" in meat
    assert (
        "playerRef.GetItemCount(RadstagMeatRaw) < RadstagMeatCount" in meat
    )
    assert "playerRef.RemoveItem(RadstagMeatRaw, RadstagMeatCount, True)" in meat
    assert "owningQuest.SetStage(StageToSetMeat)" in meat
    assert "owningQuest.SetStage(YaoGuaiStage)" in meat
    assert "owningQuest.SetStage(DeathclawStage)" in meat
    assert "TW010GrillNotReadyMsg.Show()" in meat

    assert "akActionRef != playerRef" in vegetables
    assert "owningQuest.IsStageDone(PrereqStageVegetables)" in vegetables
    assert "playerRef.GetItemCount(Tato) < TatoCount" in vegetables
    assert "playerRef.RemoveItem(Tato, TatoCount, True)" in vegetables
    assert "owningQuest.SetStage(StageToSetVegetables)" in vegetables
    assert "owningQuest.SetStage(CornStage)" in vegetables
    assert "owningQuest.SetStage(CarrotsStage)" in vegetables
    assert "TW010GrillNotReadyMsg.Show()" in vegetables


def test_tw010_objectives_markers_and_completion_are_quest_local():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    meat_turn_in = _member_body(patch, "fragment_stage_0400_item_00")
    vegetable_turn_in = _member_body(patch, "fragment_stage_0700_item_00")
    completed = _member_body(patch, "fragment_stage_1000_item_00")

    assert "SetObjectiveCompleted(300, True)" in meat_turn_in
    assert "If !IsObjectiveCompleted(120)" in meat_turn_in
    assert "SetObjectiveDisplayed(120, False)" in meat_turn_in
    assert "If !IsObjectiveCompleted(130)" in meat_turn_in
    assert "SetObjectiveDisplayed(130, False)" in meat_turn_in
    assert "TW010_TurnInMeat.Start()" in meat_turn_in
    assert "SetObjectiveCompleted(600, True)" in vegetable_turn_in
    assert "If !IsObjectiveCompleted(510)" in vegetable_turn_in
    assert "SetObjectiveDisplayed(510, False)" in vegetable_turn_in
    assert "If !IsObjectiveCompleted(520)" in vegetable_turn_in
    assert "SetObjectiveDisplayed(520, False)" in vegetable_turn_in
    assert "TW010_TurnInVeges.Start()" in vegetable_turn_in
    assert "playerRef.SetValue(TW010Status, GameDaysPassed.GetValue())" in completed
    assert "Stop()" in completed
    assert "SQ_Master" not in patch
