from __future__ import annotations

import json
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
STAGE_FLAGS_FIXTURE = (
    Path(__file__).resolve().parent / "fixtures" / "tw007_coldcase_stage_flags.json"
)

PATCH_MEMBERS = {
    "TW005BreakActivator": {
        "onaliasinit",
        "onload",
        "ensurerepairtargetregistration",
        "objectreference.onactivate",
    },
    "TW002MarshalScript": {"ondeath"},
    "TW007_PlayerScript": {
        "onaliasinit",
        "onplayerloadgame",
        "onaliasshutdown",
        "onitemadded",
        "resetfreddycluefilters",
        "reconcilefreddyclues",
    },
    "MTR02_MinerSignboardRefScript": {"onactivate"},
}


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_MEMBERS.items())
def test_progression_patches_merge_once_and_compile(
    script_name: str, expected_members: set[str]
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname" not in patch
    assert "Property" not in patch

    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected_members:
        assert members.count(member) == 1
    assert set(members) == expected_members
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


def test_tw005_repair_substitute_uses_bound_target_and_stage():
    patch = _script_patch_source("TW005BreakActivator")

    assert patch is not None
    ensure_start = patch.index("Function EnsureRepairTargetRegistration()")
    ensure_end = patch.index("EndFunction", ensure_start)
    ensure = patch[ensure_start:ensure_end]
    unregister = 'UnregisterForRemoteEvent(repairTarget, "OnActivate")'
    register = 'RegisterForRemoteEvent(repairTarget, "OnActivate")'
    assert ensure.count(unregister) == 1
    assert ensure.count(register) == 1
    assert ensure.index(unregister) < ensure.index(register)
    assert "akSender != RefToBreak.GetReference()" in patch
    assert "owningQuest.IsStageDone(200)" in patch
    assert "owningQuest.SetStage(StageToCheck)" in patch


def test_tw005_existing_active_save_registers_when_the_trigger_loads():
    patch = _script_patch_source("TW005BreakActivator")

    assert patch is not None
    assert patch.count("EnsureRepairTargetRegistration()") == 3
    assert "Event OnAliasInit()\n\tEnsureRepairTargetRegistration()" in patch
    assert "Event OnLoad()\n\tEnsureRepairTargetRegistration()" in patch


def test_tw002_marshal_death_restores_the_stage_100_producer():
    patch = _script_patch_source("TW002MarshalScript")

    assert patch is not None
    assert "Event OnDeath(Actor akKiller)" in patch
    assert "owningQuest.SetStage(100)" in patch


def test_tw007_reconciles_all_bound_clues_at_stage_1300_and_on_inventory_change():
    player_patch = _script_patch_source("TW007_PlayerScript")
    quest_patch = _script_patch_source(
        "Fragments:Quests:QF_TW007_ColdCase_002C460C"
    )

    assert player_patch is not None
    assert quest_patch is not None
    assert "playerRef.GetItemCount(FreddyClues[i].ClueForm) > 0" in player_patch
    assert "owningQuest.SetStage(FoundCluesStage)" in player_patch
    assert "Function Fragment_Stage_1300_Item_00()" in quest_patch
    assert "playerScript.ReconcileFreddyClues()" in quest_patch


def test_tw007_existing_active_save_reconciles_all_clues_on_player_load():
    player_patch = _script_patch_source("TW007_PlayerScript")

    assert player_patch is not None
    assert (
        "Event OnPlayerLoadGame()\n"
        "\tResetFreddyClueFilters()\n"
        "\tReconcileFreddyClues()"
    ) in player_patch
    assert "owningQuest.IsStageDone(PrerequisiteStage)" in player_patch
    assert "owningQuest.IsStageDone(FoundCluesStage)" in player_patch


def test_tw007_stage_1700_native_flag_owns_completion_and_stage_2000_owns_cleanup():
    evidence = json.loads(STAGE_FLAGS_FIXTURE.read_text(encoding="utf-8"))
    expected = {
        "1700": {"index_flags": [], "stage_flags": ["CompleteQuest"]},
        "2000": {"index_flags": ["RunOnStop"], "stage_flags": []},
    }
    assert evidence["source"]["record"] == "2C460C:SeventySix.esm"
    assert evidence["live"]["record"] == "002C460C"
    assert evidence["source"]["editor_id"] == "TW007_ColdCase"
    assert evidence["live"]["editor_id"] == "TW007_ColdCase"
    assert evidence["source"]["stages"] == expected
    assert evidence["live"]["stages"] == expected

    quest_patch = _script_patch_source(
        "Fragments:Quests:QF_TW007_ColdCase_002C460C"
    )
    assert quest_patch is not None
    stage_start = quest_patch.index("Function Fragment_Stage_1700_Item_00()")
    stage_end = quest_patch.index("EndFunction", stage_start)
    stage = quest_patch[stage_start:stage_end]
    assert "CompleteQuest()" not in stage
    assert "Stop()" in stage


def test_mtr02_poster_starts_through_story_manager_before_setting_stage_10():
    patch = _script_patch_source("MTR02_MinerSignboardRefScript")

    assert patch is not None
    assert "MTR02_Miner.Start()" not in patch
    assert (
        "MTR02_MinerMainQuestStartKeyword.SendStoryEventAndWait(akRef1 = akActionRef)"
        in patch
    )
    assert "MTR02_Miner.IsRunning() && !MTR02_Miner.IsStageDone(10)" in patch
    assert "MTR02_Miner.SetStage(10)" in patch
