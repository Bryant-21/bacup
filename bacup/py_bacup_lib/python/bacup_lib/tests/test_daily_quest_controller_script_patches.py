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

PATCH_MEMBERS = {
    "sq_masterscript": {
        "onquestinit",
        "getdailyquestregioncount",
        "getdailyquestselectorcount",
        "rebuilddailyquestsequence",
        "getselecteddailyquest",
        "initializedailyquestschedule",
        "advancedailyquestschedule",
        "resetbettertomorrowdaily",
        "trystartbettertomorrow",
        "refreshcostabusinesseligibility",
        "updatedailyquestschedule",
        "trystartdailyquest",
    },
    "RegionManagerScript": {
        "startdailyquestpulse",
        "registerdailyquestevents",
        "rundailyquestcontroller",
        "onquestinit",
        "actor.onplayerloadgame",
        "actor.onlocationchange",
        "ontimergametime",
    },
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
def test_daily_controller_patch_is_member_only_and_complete(
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
def test_daily_controller_patch_merges_once(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member in members:
        assert names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_daily_controller_full_production_sources_native_compile(tmp_path: Path):
    merged_master = _merged_production_source("sq_masterscript")
    merged_region_manager = _merged_production_source("RegionManagerScript")
    merged_source_root = tmp_path / "Scripts" / "Source" / "User"
    merged_source_root.mkdir(parents=True)
    (merged_source_root / "sq_masterscript.psc").write_text(
        merged_master, encoding="utf-8"
    )
    (merged_source_root / "RegionManagerScript.psc").write_text(
        merged_region_manager, encoding="utf-8"
    )

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    for script_name, merged in (
        ("sq_masterscript", merged_master),
        ("RegionManagerScript", merged_region_manager),
    ):
        result = compile_psc(
            merged,
            imports=[
                str(merged_source_root),
                str(SOURCE_ROOT),
                str(base_source),
            ],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )

        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, diagnostics
        assert result.pex_bytes is not None


def test_daily_controller_uses_only_the_six_retained_legacy_selectors():
    patch = _script_patch_source("sq_masterscript")
    assert patch is not None

    region_count = _member_body(patch, "getdailyquestregioncount")
    selector_count = _member_body(patch, "getdailyquestselectorcount")
    sequence = _member_body(patch, "rebuilddailyquestsequence")

    assert "Regions.Length < 6" in region_count
    assert "Return 6" in region_count
    assert selector_count.count("Return 2") == 3
    assert "Return 3" in selector_count
    assert "Return 4" in selector_count
    assert "Return 5" in selector_count
    assert "dailyQuestMasterList.GetSize() < questCount" in sequence
    assert "questIndex < questCount" in sequence
    assert "MTR04_Employee" not in patch
    assert "W05_Daily_Photo" not in patch


def test_sq_master_boots_the_bound_region_manager_quest():
    patch = _script_patch_source("sq_masterscript")
    assert patch is not None

    quest_init = _member_body(patch, "onquestinit")

    assert "SQ_RegionManager != None" in quest_init
    assert "!SQ_RegionManager.IsRunning()" in quest_init
    assert "SQ_RegionManager.Start()" in quest_init


def test_daily_controller_advances_by_game_day_without_stopping_active_quests():
    patch = _script_patch_source("sq_masterscript")
    assert patch is not None

    update = _member_body(patch, "updatedailyquestschedule")
    advance = _member_body(patch, "advancedailyquestschedule")

    assert "(Utility.GetCurrentGameTime() as Int) + 1" in update
    assert "SQ_TimestampToday.GetValueInt()" in update
    assert "today > previousDay" in update
    assert "elapsedDays = aiToday - aiPreviousDay" in advance
    assert "(previousIndex + elapsedDays) % questCount" in advance
    assert "previousQuest.IsCompleted() || previousQuest.IsStopped()" in advance
    assert "previousQuest.Reset()" in advance
    assert ".Stop()" not in patch


def test_daily_controller_uses_current_lists_as_once_per_day_story_guards():
    patch = _script_patch_source("sq_masterscript")
    assert patch is not None

    initialize = _member_body(patch, "initializedailyquestschedule")
    advance = _member_body(patch, "advancedailyquestschedule")
    start = _member_body(patch, "trystartdailyquest")

    assert "dailyQuestListCurrent.Revert()" in initialize
    assert "dailyQuestListCurrent.AddForm(selectedQuest)" in initialize
    assert "dailyQuestListCurrent.Revert()" in advance
    assert "dailyQuestListCurrent.HasForm(selectedQuest)" in start
    assert "dailyQuestListCurrent.RemoveAddedForm(selectedQuest)" in start
    assert "selectedQuest.IsRunning()" in start
    assert "selectedQuest.Reset()" in start
    assert (
        "akStoryKeyword.SendStoryEventAndWait("
        "currentRegion.regionLocation, playerRef)"
    ) in start
    assert "akStoryKeyword.SendStoryEvent(" not in start
    assert "!startedDailyQuest" in start
    assert "GetSelectedDailyQuest(regionIndex) == selectedQuest" in start
    assert "!currentRegion.dailyQuestListCurrent.HasForm(selectedQuest)" in start
    assert "currentRegion.dailyQuestListCurrent.AddForm(selectedQuest)" in start
    assert start.index("RemoveAddedForm(selectedQuest)") < start.index(
        "SendStoryEventAndWait"
    )
    assert start.index("SendStoryEventAndWait") < start.index(
        "AddForm(selectedQuest)"
    )
    assert ".Start(" not in start


def test_better_tomorrow_uses_its_exact_regional_story_manager_contract():
    patch = _script_patch_source("sq_masterscript")
    assert patch is not None

    initialize = _member_body(patch, "initializedailyquestschedule")
    advance = _member_body(patch, "advancedailyquestschedule")
    reset = _member_body(patch, "resetbettertomorrowdaily")
    better_tomorrow = _member_body(patch, "trystartbettertomorrow")
    start = _member_body(patch, "trystartdailyquest")

    assert 'Game.GetFormFromFile(0x006FD072, "SeventySix.esm") as Quest' in reset
    assert 'Game.GetFormFromFile(0x006FD072, "SeventySix.esm") as Quest' in better_tomorrow
    assert 'Game.GetFormFromFile(0x0072C956, "SeventySix.esm") as Keyword' in better_tomorrow
    assert 'Game.GetFormFromFile(0x00095004, "SeventySix.esm") as Location' in better_tomorrow
    assert 'Game.GetFormFromFile(0x0009B044, "Fallout4.esm") as Keyword' in better_tomorrow
    assert "akPlayerLocation.IsSameLocation(betterTomorrowRegion, regionLocationType)" in better_tomorrow
    assert "betterTomorrowStartKeyword.SendStoryEventAndWait(betterTomorrowRegion, akPlayerRef)" in better_tomorrow
    assert ".Start(" not in better_tomorrow
    assert "ResetBetterTomorrowDaily()" in initialize
    assert "ResetBetterTomorrowDaily()" in advance
    assert "TryStartBetterTomorrow(akPlayerLocation, playerRef)" in start
    assert "0x006FD072" not in _member_body(patch, "rebuilddailyquestsequence")


def test_region_manager_registers_player_events_and_rearms_game_time_pulse():
    patch = _script_patch_source("RegionManagerScript")
    assert patch is not None

    register = _member_body(patch, "registerdailyquestevents")
    pulse = _member_body(patch, "startdailyquestpulse")
    location_change = _member_body(patch, "actor.onlocationchange")
    player_load = _member_body(patch, "actor.onplayerloadgame")
    timer = _member_body(patch, "ontimergametime")

    assert 'RegisterForRemoteEvent(playerRef, "OnLocationChange")' in register
    assert 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in register
    assert "SQ_RegionManagerPulseTimeSeconds.GetValue()" in pulse
    assert "pulseHours = pulseSeconds / 3600.0" in pulse
    assert "StartTimerGameTime(pulseHours, pulseDelayTimerID)" in pulse
    assert "StartTimer(" not in patch
    assert "Event OnTimer(" not in patch
    assert "RunDailyQuestController(akNewLoc)" in location_change
    assert "RunDailyQuestController(akSender.GetCurrentLocation())" in player_load
    assert "If aiTimerID == pulseDelayTimerID" in timer
    assert "StartDailyQuestPulse()" in timer


def test_daily_controller_logs_every_runtime_decision_layer():
    master = _script_patch_source("sq_masterscript")
    region_manager = _script_patch_source("RegionManagerScript")
    assert master is not None
    assert region_manager is not None

    assert "[B21 Daily] RegionManager OnQuestInit" in region_manager
    assert "RegionManager controller tick location=" in region_manager
    assert "RegionManager SQ_Master cast failed" in region_manager
    assert "RegionManager OnLocationChange" in region_manager
    assert "RegionManager game-time pulse fired" in region_manager

    assert "SQ_Master TryStart location=" in master
    assert "SQ_Master region check index=" in master
    assert "SQ_Master matched region index=" in master
    assert "SQ_Master selected quest is not eligible today" in master
    assert "SQ_Master Story Manager result=" in master
    assert "SQ_Master found no matching region" in master
