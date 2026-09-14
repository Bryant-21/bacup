from __future__ import annotations

from pathlib import Path
import re

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
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "wild-ac-empty-quests-gap-repair-2026-09-03.md"
)

PATCHED_SCRIPTS = (
    "Fragments:Quests:QF_P01B_Master_0047F444",
    "Fragments:Quests:QF_P01B_Mini_Squatch01_0046484F",
    "Fragments:Quests:QF_P01B_Mini_Robot01_0046DC6F",
    "Fragments:Quests:QF_P01B_Mini_Random01_0047F441",
    "Fragments:Quests:QF_P01B_Mini_Albino01_0046D628",
    "Fragments:Quests:QF_P01B_Mini_Random03_0046A5A9",
    "Fragments:Quests:QF_P01B_Mini_Random04_00460047",
    "Fragments:Quests:QF_P01B_Mini_Random02_0047F442",
    "Fragments:Quests:QF_M01C_Atheletics_004178BE",
    "Fragments:Quests:QF_M01C_Athletics_Bridge_004266B7",
    "Fragments:Quests:QF_M01C_Athletics_Venture_004266B8",
    "Fragments:Quests:QF_M01C_Athletics_Slopes_00536BE1",
    "Fragments:Quests:QF_TestUD002_003ECC12",
    "Quests:UD002:UD002Quest",
    "Quests:M01C:Athletics_QuestScript",
    "P01b_Mini_Random02_Script",
    "P01B_Mini_Random04_Script",
)


def _merged_script(script_name: str) -> tuple[str, str]:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    ), patch


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_wild_ac_gap_patches_merge_once_and_compile(script_name: str):
    merged, patch = _merged_script(script_name)
    assert "Scriptname" not in patch
    assert not re.search(r"\bProperty\b", patch)

    patch_members = {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    merged_members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    assert patch_members
    for member in patch_members:
        assert merged_members.count(member) == 1, (script_name, member)
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


def test_event_scoped_quests_keep_story_manager_routing():
    athletics_master, _ = _merged_script(
        "Fragments:Quests:QF_M01C_Atheletics_004178BE"
    )
    burrows, _ = _merged_script("Fragments:Quests:QF_TestUD002_003ECC12")
    p01b_master, _ = _merged_script("Fragments:Quests:QF_P01B_Master_0047F444")

    assert "ventureStart.SendStoryEventAndWait" in athletics_master
    assert "StartKeyword_M01C_Athletics_Bridge.SendStoryEventAndWait" in athletics_master
    assert "slopesStart.SendStoryEventAndWait" in athletics_master
    assert "UD002_BossWave_Keyword.SendStoryEventAndWait" in burrows
    assert p01b_master.count("SendStoryEventAndWait") >= 9

    direct_quest_starts = (
        "Quest_Venture.Start()",
        "Quest_Bridge.Start()",
        "Quest_Slopes.Start()",
        "UD002_Burrows.Start()",
    )
    for source in (athletics_master, burrows, p01b_master):
        assert all(call not in source for call in direct_quest_starts)


def test_p01b_wrapper_credit_and_completion_paths_are_restored():
    master, _ = _merged_script("Fragments:Quests:QF_P01B_Master_0047F444")
    expected_wrapper_stages = {
        2000: 41,
        2010: 42,
        3000: 31,
        3010: 32,
        4000: 51,
        4010: 52,
        5000: 11,
        5010: 12,
        6000: 21,
        6010: 22,
        7000: 61,
        7010: 62,
        8000: 71,
        8010: 72,
    }
    for source_stage, wrapper_stage in expected_wrapper_stages.items():
        member = f"Function Fragment_Stage_{source_stage:04d}_Item_00()"
        body_start = master.index(member)
        body_end = master.index("EndFunction", body_start)
        assert f"SetStage({wrapper_stage})" in master[body_start:body_end]

    for script_name in PATCHED_SCRIPTS[1:8]:
        merged, _ = _merged_script(script_name)
        assert "Function Fragment_Stage_9000_Item_00()" in merged
        assert "CompleteQuest()" in merged


def test_athletics_checkpoint_percentages_and_terminal_stages_are_complete():
    expected = {
        "Fragments:Quests:QF_M01C_Athletics_Bridge_004266B7": (300, 315, 100),
        "Fragments:Quests:QF_M01C_Athletics_Venture_004266B8": (200, 212, 100),
        "Fragments:Quests:QF_M01C_Athletics_Slopes_00536BE1": (400, 410, 100),
    }
    for script_name, (first_stage, final_stage, final_percent) in expected.items():
        merged, _ = _merged_script(script_name)
        assert f"Fragment_Stage_{first_stage:04d}_Item_00" in merged
        assert f"Fragment_Stage_{final_stage:04d}_Item_00" in merged
        assert f"SetValue(AV_ProgressCurrent, {final_percent}.0)" in merged
        assert "SetStage(9000)" in merged
        assert "Quest_M01C_Athletics.SetStage(900)" in merged
        assert "Quest_M01C_Athletics.SetStage(800)" in merged


def test_burrows_controller_is_local_and_ac03_remains_explicitly_blocked():
    burrows, _ = _merged_script("Fragments:Quests:QF_TestUD002_003ECC12")
    controller, _ = _merged_script("Quests:UD002:UD002Quest")
    contract = CONTRACT.read_text(encoding="utf-8")

    assert "StartLocalEncounterWave(0)" in burrows
    assert "StartLocalEncounterWave(1)" in burrows
    assert "bossFurniture.PlaceActorAtMe(Assaultron, 3)" in controller
    assert "Boss.ForceRefTo(SpawnedBossActor)" in controller
    assert "BossCollection.AddRef(SpawnedBossActor)" in controller
    assert _script_patch_source(
        "Fragments:Quests:QF_XPD_AC03_Mission_Human_006DBABB"
    ) is None
    assert _script_patch_source("XPD_AC03_Mission_Human_QuestScript") is None
    assert "unsupported-online-service" in contract
    assert "006DBABB" in contract
    assert "004825A6" in contract


def test_athletics_master_selects_a_reachable_forced_instructor():
    controller, _ = _merged_script("Quests:M01C:Athletics_QuestScript")
    fragment, _ = _merged_script(
        "Fragments:Quests:QF_M01C_Atheletics_004178BE"
    )

    assert "Event OnQuestInit()" in controller
    assert "GetAlias(139)" not in controller
    assert "instructorAliasID = 139" in controller
    assert "instructorAliasID = 153" in controller
    assert "instructorAliasID = 171" in controller
    assert "instructorRef.Is3DLoaded()" in controller
    assert "playerRef.GetDistance(instructorRef)" in controller
    assert "currentInstructor.ForceRefTo(selectedInstructor)" in controller
    assert "athleticsController.SelectCurrentInstructor()" in fragment
    assert "SetStage(200)" in fragment
    assert "SetStage(300)" in fragment
    assert "SetStage(400)" in fragment


def test_random02_den_distance_event_reaches_stage_500():
    controller, _ = _merged_script("P01b_Mini_Random02_Script")

    assert "Event OnQuestInit()" in controller
    assert "ObjectReference denMarker = alias_DenMarker.GetReference()" in controller
    assert "denMarker.Is3DLoaded() && playerRef.GetDistance(denMarker) <= Distance" in controller
    assert "RegisterForDistanceLessThanEvent(playerRef, denMarker, Distance)" in controller
    assert "Event OnDistanceLessThan" in controller
    assert "SetStage(500)" in controller
    assert "UnregisterForDistanceEvents(playerRef, denMarker)" in controller


def test_random04_all_recorded_clues_reach_stage_550():
    controller, _ = _merged_script("P01B_Mini_Random04_Script")
    fragment, _ = _merged_script(
        "Fragments:Quests:QF_P01B_Mini_Random04_00460047"
    )

    for stage in (10, 11, 12, 13, 21, 22, 400):
        assert f"IsStageDone({stage})" in controller
    assert "locationOneClues >= MaxCluesLoc1" in controller
    assert "locationTwoClues >= MaxCluesLoc2" in controller
    assert "locationThreeClues >= MaxCluesLoc3" in controller
    assert "SetStage(550)" in controller
    assert "ElseIf !IsStageDone(500)" in controller
    assert fragment.count("questController.RefreshClueProgress()") == 7
    stage_400_start = fragment.index("Function Fragment_Stage_0400_Item_00()")
    stage_400_end = fragment.index("EndFunction", stage_400_start)
    stage_400 = fragment[stage_400_start:stage_400_end]
    assert "RefreshClueProgress()" in stage_400
    assert "SetStage(500)" not in stage_400
