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
DROSS = "Quests:MTR04:Dross"
LUCKY = "Quests:MTR04:Lucky"
CHOW = "Quests:MTR04:Chow"
CHOW_MASTER = "Quests:MTR04:Chow_Master"
EMPLOYEE = "mtr04_gamescomplete"
DROSS_FRAGMENT = "Fragments:Quests:QF_MTR04_Dross_00320E08"
LUCKY_FRAGMENT = "Fragments:Quests:QF_MTR04_Lucky_00320E0A"
CHOW_FRAGMENT = "Fragments:Quests:QF_MTR04_Chow_00320E09"
EMPLOYEE_FRAGMENT = "Fragments:Quests:QF_MTR04_Games_001ED40A"
DROSS_FAILURE = "Fragments:Scenes:SF_MTR04_Dross_Failure_004E20F0"
DROSS_SUCCESS = "Fragments:Scenes:SF_MTR04_Dross_Success_004E20E6"
CHOW_FAILURE = "Fragments:Scenes:SF_MTR04_Chow_Failure_004E23AF"
CHOW_SUCCESS = "Fragments:Scenes:SF_MTR04_Chow_Success_004E23C9"

PATCH_MEMBERS = {
    DROSS: {
        "onquestinit",
        "beginthrowing",
        "scoregame",
        "objectreference.onitemremoved",
        "objectreference.ontriggerenter",
        "actor.onitemequipped",
        "actor.onitemunequipped",
        "ontimer",
        "onquestshutdown",
    },
    LUCKY: {
        "onquestinit",
        "onstageset",
        "actor.onitemequipped",
        "actor.onitemunequipped",
        "onquestshutdown",
    },
    CHOW_MASTER: {"preparehotdogs", "rotatehotdog"},
    CHOW: {
        "onquestinit",
        "begineating",
        "finisheating",
        "objectreference.onactivate",
        "ontimer",
        "actor.onitemequipped",
        "actor.onitemunequipped",
        "onquestshutdown",
    },
    EMPLOYEE: {
        "onquestinit",
        "resetactivitycompletion",
        "activitycompleted",
        "objectreference.onitemadded",
        "actor.onitemequipped",
        "onquestshutdown",
    },
    DROSS_FRAGMENT: {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0201_item_00",
        "fragment_stage_0202_item_00",
        "fragment_stage_0250_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0310_item_00",
        "fragment_stage_0320_item_00",
        "fragment_stage_0330_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_1500_item_00",
    },
    LUCKY_FRAGMENT: {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0201_item_00",
        "fragment_stage_0202_item_00",
        "fragment_stage_0250_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_1500_item_00",
    },
    CHOW_FRAGMENT: {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0201_item_00",
        "fragment_stage_0202_item_00",
        "fragment_stage_0250_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_1500_item_00",
    },
    EMPLOYEE_FRAGMENT: {
        "fragment_stage_0001_item_00",
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_0750_item_00",
        "fragment_stage_0800_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_1001_item_00",
        "fragment_stage_2000_item_00",
    },
    DROSS_FAILURE: {"fragment_phase_01_end"},
    DROSS_SUCCESS: {"fragment_phase_01_end"},
    CHOW_FAILURE: {"fragment_phase_01_end"},
    CHOW_SUCCESS: {"fragment_phase_01_end"},
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
def test_mtr04_patch_is_member_only_and_complete(
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
def test_mtr04_patch_merges_once_and_native_compiles(
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


def test_dross_counts_five_throws_and_scores_unique_tire_hits():
    patch = _script_patch_source(DROSS)
    fragment = _script_patch_source(DROSS_FRAGMENT)
    assert patch is not None
    assert fragment is not None

    begin = _member_body(patch, "beginthrowing")
    removed = _member_body(patch, "objectreference.onitemremoved")
    target = _member_body(patch, "objectreference.ontriggerenter")
    score = _member_body(patch, "scoregame")

    assert "AddItem(DrossGrenade, NumberOfChancesToThrow, True)" in begin
    assert "numberOfDrossThrown += aiItemCount" in removed
    assert "StartTimer(SecondsToWaitOnLastThrow" in removed
    assert "!IsStageDone(target.StageToSetWhenHit)" in target
    assert "SetValue(TargetsHitRewardAV, numberOfTargetsHit as Float)" in target
    assert "SetStage(SuccessStage)" in score
    assert "SetStage(FailureStage)" in score
    assert "drossQuest.ScoreGame()" in _member_body(
        fragment, "fragment_stage_0400_item_00"
    )


def test_lucky_counts_each_retained_cart_stage_once():
    patch = _script_patch_source(LUCKY)
    fragment = _script_patch_source(LUCKY_FRAGMENT)
    assert patch is not None
    assert fragment is not None

    stages = _member_body(patch, "onstageset")
    assert "auiStageID >= 310 && auiStageID <= 340" in stages
    assert "currentCartsActivated += 1" in stages
    assert "currentCartsActivated >= Carts.Length" in stages
    assert "SetStage(completeStage)" in stages
    assert "LuckySuccess.Start()" in _member_body(
        fragment, "fragment_stage_0500_item_00"
    )


def test_chow_rotates_world_hotdogs_and_enforces_digest_cooldown():
    chow = _script_patch_source(CHOW)
    master = _script_patch_source(CHOW_MASTER)
    assert chow is not None
    assert master is not None

    activate = _member_body(chow, "objectreference.onactivate")
    digest = _member_body(chow, "ontimer")
    rotate = _member_body(master, "rotatehotdog")

    assert "GetValue(MTR04_CanEatHotdog) < 1.0" in activate
    assert "numOfHotdogsEaten += 1" in activate
    assert "numOfHotdogsEaten >= RequiredHotdogsEatenCount" in activate
    assert "HotdogEffects[effectIndex].Cast(playerRef, playerRef)" in activate
    assert "StartTimer(Utility.RandomFloat(DigestingTimeMin" in activate
    assert "SetValue(MTR04_CanEatHotdog, 1.0)" in digest
    assert "Utility.RandomFloat(HotdogEnableTimeMin, HotdogEnableTimeMax)" in rotate


def test_mistaken_identity_starts_all_three_games_and_counts_each_once():
    tracker = _script_patch_source(EMPLOYEE)
    fragment = _script_patch_source(EMPLOYEE_FRAGMENT)
    assert tracker is not None
    assert fragment is not None

    stage_800 = _member_body(fragment, "fragment_stage_0800_item_00")
    completion = _member_body(tracker, "activitycompleted")
    assert "DrossQuest.Start()" in stage_800
    assert "LuckyQuest.Start()" in stage_800
    assert "ChowQuest.Start()" in stage_800
    assert "activityTracker.ActivityCompleted(0)" in stage_800
    assert "activityTracker.ActivityCompleted(1)" in stage_800
    assert "activityTracker.ActivityCompleted(2)" in stage_800
    assert "ActivityCompletionStatus[aiActivityIndex] = True" in completion
    assert "SetStage(StageToSet)" in completion
    assert "CompleteQuest()" in _member_body(
        fragment, "fragment_stage_2000_item_00"
    )


@pytest.mark.parametrize(
    "script_name",
    [DROSS_FAILURE, DROSS_SUCCESS, CHOW_FAILURE, CHOW_SUCCESS],
)
def test_mtr04_scene_end_fragments_stop_their_owning_quest(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None

    phase_end = _member_body(patch, "fragment_phase_01_end")
    assert "GetOwningQuest()" in phase_end
    assert "owningQuest.Stop()" in phase_end
