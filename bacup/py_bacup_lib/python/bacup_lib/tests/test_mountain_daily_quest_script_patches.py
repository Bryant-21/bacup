from __future__ import annotations

import re
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

MTNS_QUEST_SCRIPT = "MTNS05QuestScript"
MTNS_FRAGMENT = "Fragments:Quests:QF_MTNS05_Voices_000043A2"
PUMPKIN_QUEST_SCRIPT = "Quests:LC129:PumpkinQuestScript"
PUMPKIN_FRAGMENT = (
    "Fragments:Quests:QF_LC129_Pumpkin_Grenade_Que_00137931"
)
PUMPKIN_TURN_INS = (
    "Fragments:TopicInfos:TIF_LC129_TrickOrTreat_0052B651",
    "Fragments:TopicInfos:TIF_LC129_TrickOrTreat_0052B653",
)

PATCH_MEMBERS = {
    "MTNS05_VoxScript": {"oneffectstart"},
    MTNS_QUEST_SCRIPT: {
        "resettargets",
        "initializetargets",
        "assigntarget",
        "handlevoxtarget",
        "resettargetattempt",
        "completetarget",
        "ontimer",
    },
    MTNS_FRAGMENT: {
        "fragment_stage_0025_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0050_item_01",
        "fragment_stage_0100_item_00",
        "fragment_stage_0110_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_0160_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0275_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
        "objectreference.oncontainerchanged",
    },
    PUMPKIN_QUEST_SCRIPT: {"tryturninpumpkins"},
    PUMPKIN_FRAGMENT: {
        "fragment_stage_0001_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_1000_item_00",
    },
    PUMPKIN_TURN_INS[0]: {"fragment_begin"},
    PUMPKIN_TURN_INS[1]: {"fragment_begin"},
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
def test_mountain_daily_patch_is_member_only_and_complete(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} "
        for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_mountain_daily_patch_merges_once(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member in members:
        assert names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_mtns05_terminal_patch_preserves_named_state_dispatch():
    patch = _script_patch_source("MTNS05_TerminalScript")
    assert patch is not None
    merged = _merged_production_source("MTNS05_TerminalScript")

    assert patch.count("State init") == 1
    assert patch.count("State terminalaccessed") == 1
    assert merged.count("State init") == 1
    assert merged.count("State terminalaccessed") == 1
    assert re.search(
        r"State init\b.*?Event OnActivate\(ObjectReference akActivator\).*?"
        r"owningQuest\.SetStage\(TerminalAccessedStage\).*?EndState",
        merged,
        re.IGNORECASE | re.DOTALL,
    )
    assert _merge_script_method_patches(merged, patch) == merged


def test_mountain_daily_merged_sources_native_compile(tmp_path: Path):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    patched_imports = tmp_path / "patched_imports"
    patched_imports.mkdir()
    for script_name in (MTNS_QUEST_SCRIPT, PUMPKIN_QUEST_SCRIPT):
        source_path = patched_imports / _script_relative_path(script_name, ".psc")
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(
            _merged_production_source(script_name), encoding="utf-8"
        )

    scripts = (
        "MTNS05_TerminalScript",
        MTNS_QUEST_SCRIPT,
        "MTNS05_VoxScript",
        MTNS_FRAGMENT,
        PUMPKIN_QUEST_SCRIPT,
        PUMPKIN_FRAGMENT,
        *PUMPKIN_TURN_INS,
    )
    for script_name in scripts:
        merged = _merged_production_source(script_name)
        result = compile_psc(
            merged,
            imports=[
                str(patched_imports),
                str(SOURCE_ROOT),
                str(base_source),
            ],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}\n{diagnostics}"
        assert result.pex_bytes is not None


def test_mtns05_first_run_repeat_and_target_collection_contract():
    terminal = _script_patch_source("MTNS05_TerminalScript")
    quest = _script_patch_source(MTNS_QUEST_SCRIPT)
    fragment = _script_patch_source(MTNS_FRAGMENT)
    effect = _script_patch_source("MTNS05_VoxScript")
    assert terminal is not None
    assert quest is not None
    assert fragment is not None
    assert effect is not None

    assert "GetStage() >= TerminalAccessedStage" in terminal
    assert "owningQuest.SetStage(TerminalAccessedStage)" in terminal
    assert "Utility.RandomInt(0, CreatureData.Length - 1)" in quest
    assert "AssignTarget(0, CreatureData[firstIndex], QTArea01)" in quest
    assert "CurrentTargetData[index].CurrentTargetRace == akTarget.GetRace()" in (
        quest
    )
    assert "StartTimer(collectionTime, 100 + index)" in quest
    assert "targetActor.IsDead()" in quest
    assert "targetDistance > fTargetDistance01" in quest
    assert "SetStage(ObjectivesDoneStage)" in quest
    assert "voicesQuest.HandleVoxTarget(akTarget, self)" in effect

    first_run = _member_body(fragment, "fragment_stage_0025_item_00")
    repeat = _member_body(fragment, "fragment_stage_0050_item_00")
    setup = _member_body(fragment, "fragment_stage_0200_item_00")
    return_syringer = _member_body(
        fragment, "objectreference.oncontainerchanged"
    )
    finish = _member_body(fragment, "fragment_stage_0400_item_00")
    assert "SetObjectiveDisplayed(25)" in first_run
    assert "SetObjectiveDisplayed(50)" in repeat
    assert "MTNS05_Voices.SetStage(150)" in fragment
    assert "voicesQuest.InitializeTargets()" in setup
    assert "akNewContainer == Alias_SyringerContainer.GetReference()" in (
        return_syringer
    )
    assert "MTNS05_Voices.SetStage(300)" in return_syringer
    assert "playerRef.RemoveItem(holotapeRef, 1, true)" in finish


def test_mtns05_completion_unregisters_before_stopping():
    fragment = _merged_production_source(MTNS_FRAGMENT)
    acquire_syringer = _member_body(
        fragment, "fragment_stage_0150_item_00"
    )
    finish = _member_body(fragment, "fragment_stage_0400_item_00")

    assert (
        'RegisterForRemoteEvent(syringerRef, "OnContainerChanged")'
        in acquire_syringer
    )
    ordered_calls = (
        "MTNS05_Voices.SetObjectiveCompleted(300)",
        'UnregisterForRemoteEvent(syringerRef, "OnContainerChanged")',
        "playerRef.RemoveItem(holotapeRef, 1, true)",
        "MTNS05_Voices.Stop()",
    )
    positions = [finish.index(call) for call in ordered_calls]
    assert positions == sorted(positions)
    assert re.search(r"MTNS05_Voices\.Stop\(\)\s*\nEndFunction$", finish)


def test_mtns05_rejects_a_second_actor_for_an_active_target_slot():
    quest = _merged_production_source(MTNS_QUEST_SCRIPT)
    handle_target = _member_body(quest, "handlevoxtarget")

    occupied_slot_guard = (
        "CurrentTargetData[index].CurrentTargetActor == None"
    )
    assign_actor = "CurrentTargetData[index].CurrentTargetActor = akTarget"
    start_timer = "StartTimer(collectionTime, 100 + index)"
    assert occupied_slot_guard in handle_target
    assert (
        handle_target.index(occupied_slot_guard)
        < handle_target.index(assign_actor)
        < handle_target.index(start_timer)
    )


def test_lc129_prompt_pickup_and_turn_in_contract():
    quest = _script_patch_source(PUMPKIN_QUEST_SCRIPT)
    fragment = _script_patch_source(PUMPKIN_FRAGMENT)
    assert quest is not None
    assert fragment is not None

    assert "playerRef.GetItemCount(PumpkinVegetable) >= 10" in quest
    assert "SetStage(600)" in quest
    start = _member_body(fragment, "fragment_stage_0001_item_00")
    approach = _member_body(fragment, "fragment_stage_0200_item_00")
    collection = _member_body(fragment, "fragment_stage_0300_item_00")
    turn_in = _member_body(fragment, "fragment_stage_0600_item_00")
    assert "Alias_QuestPlayer.ForceRefTo(playerRef)" in start
    assert "GetOwningQuest().SetObjectiveDisplayed(10)" in start
    assert "WelcomeScene.Start()" in approach
    assert "inventoryManager.EvaluateInventoryState()" in collection
    assert "playerRef.RemoveItem(PumpkinItem, pumpkinsToRemove, true)" in turn_in
    assert "playerRef.SetValue(LC129_DailyCompletedFirstTime, 1.0)" in turn_in

    for script_name in PUMPKIN_TURN_INS:
        info = _script_patch_source(script_name)
        assert info is not None
        assert "pumpkinQuest.TryTurnInPumpkins()" in info


def test_lc129_completion_stops_quest_after_scene_cleanup():
    fragment = _merged_production_source(PUMPKIN_FRAGMENT)
    finish = _member_body(fragment, "fragment_stage_1000_item_00")

    ordered_calls = (
        "owningQuest.CompleteAllObjectives()",
        "GoodbyeScene.Stop()",
        "owningQuest.Stop()",
    )
    positions = [finish.index(call) for call in ordered_calls]
    assert positions == sorted(positions)
    assert re.search(r"owningQuest\.Stop\(\)\s*\nEndFunction$", finish)
