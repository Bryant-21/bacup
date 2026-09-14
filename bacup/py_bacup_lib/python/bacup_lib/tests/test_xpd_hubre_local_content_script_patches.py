from __future__ import annotations

import re
from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_script_patch_conventions import (
    has_unregistered_inventory_handler,
    sibling_script_casts,
)
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

BASE = "Quests:XPD_HubRE:XPD_HubRE_QuestScript"
LOST = "Quests:XPD_HubRE:XPD_HubRE_LostAndFound_QuestScript"
REPAIR = "Quests:XPD_HubRE:XPD_HubRE_RepairRobot_QuestScript"
PHOTO = "Quests:XPD_HubRE:XPD_HubRE_TakePhoto_QuestScript"
REPAIR_MESSAGE = "Quests:XPD_HubRE:XPD_HubRE_ShowMessageAlias"

CAMERA_SHY = "fragments:quests:qf_xpd_hubre_takephoto_camer_00646e8b"
CHESS = "fragments:quests:qf_xpd_hubre_lostandfound_ch_006422f9"
CONVERSION = "Fragments:Quests:QF_XPD_HubRE_RepairRobot_Con_006453B9"
DOG = "Fragments:Quests:QF_XPD_HubRE_RepairRobot_Dog_00647FC1"
GARBAGE = "Fragments:Quests:QF_XPD_HubRE_Template_Settle_0063272C"
HONOR = "Fragments:Quests:QF_XPD_HubRE_LostAndFound_Ye_00645790"
HOTEL = "Fragments:Quests:QF_XPD_HubRE_Template_TakePh_00633790"
IMPOSTOR = "Fragments:Quests:QF_XPD_HubRE_TakePhoto_Impos_00649ED9"
LETTERS = "Fragments:Quests:QF_XPD_HubRE_TakePhoto_Lette_0064BAD0"
LOVERS = "Fragments:Quests:QF_XPD_HubRE_SettleDispute_L_0064A88E"
MAKEOVER = "Fragments:Quests:QF_XPD_HubRE_RepairRobot_MrM_0064ACD7"
SURPRISES = "Fragments:Quests:QF_XPD_HubRE_TakePhoto_NoSur_006454DF"
READING = "Fragments:Quests:QF_XPD_HubRE_LostAndFound_Re_00643EB9"
RECRUITMENT = "Fragments:Quests:QF_XPD_HubRE_SettleDispute_R_0064A14E"
FUSSFUNGLE = "Fragments:Quests:QF_XPD_HubRE_RepairRobot_Fus_00645D90"
TOOTHACHE = "Fragments:Quests:QF_XPD_HubRE_RepairRobot_Too_00647A3E"
SHOWDOWN = "Fragments:Quests:QF_XPD_HubRE_SettleDispute_U_006478A4"
WEAPON = "Fragments:Quests:QF_XPD_HubRE_SettleDispute_W_0064DBEF"

QFS = (
    CAMERA_SHY,
    CHESS,
    CONVERSION,
    DOG,
    GARBAGE,
    HONOR,
    HOTEL,
    IMPOSTOR,
    LETTERS,
    LOVERS,
    MAKEOVER,
    SURPRISES,
    READING,
    RECRUITMENT,
    FUSSFUNGLE,
    TOOTHACHE,
    SHOWDOWN,
    WEAPON,
)
PATCHED_SCRIPTS = (BASE, LOST, REPAIR, PHOTO, REPAIR_MESSAGE, *QFS)

BOUND_FRAGMENT_STAGES = {
    CAMERA_SHY: (100, 200, 250, 300, 3000, 8000, 9000),
    CHESS: (100, 200, 250, 299, 8000, 9000),
    CONVERSION: (100, 200, 250, 300, 3000, 8000, 9000),
    DOG: (100, 200, 250, 300, 3000, 8000, 8100, 8200, 9000),
    GARBAGE: (100, 150, 299, 9000),
    HONOR: (100, 200, 210, 250, 260, 8000, 9000),
    HOTEL: (100, 200, 205, 300, 8000, 9000),
    IMPOSTOR: (100, 200, 250, 300, 3000, 8000, 9000),
    LETTERS: (
        100,
        200,
        225,
        300,
        1500,
        2000,
        3000,
        3100,
        3200,
        3300,
        4000,
        4100,
        4200,
        4300,
        5000,
        5100,
        5200,
        5300,
        8000,
        9000,
    ),
    LOVERS: (100, 101, 102, 103, 150, 200, 2100, 2200, 2300, 9000),
    MAKEOVER: (100, 200, 210, 220, 250, 300, 310, 350, 8000, 9000),
    SURPRISES: (
        100,
        200,
        220,
        2000,
        2100,
        2200,
        3000,
        3100,
        4200,
        4300,
        8000,
        9000,
        9100,
    ),
    READING: (100, 200, 250, 299, 8000, 9000),
    RECRUITMENT: (100, 150, 299, 2000, 9000),
    FUSSFUNGLE: (100, 200, 250, 300, 3000, 8000, 8100, 9000),
    TOOTHACHE: (100, 200, 210, 250, 300, 3000, 7000, 8000, 8100, 9000),
    SHOWDOWN: (100, 150, 299, 9000),
    WEAPON: (100, 150, 299, 9000),
}


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _members(source: str) -> list[tuple[str, int, int]]:
    return [
        (name, start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for name, start, end in _members(source)
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch(script_name)
    )


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_hubre_patch_is_member_only_unique_and_idempotent(script_name: str) -> None:
    patch = _patch(script_name)
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )
    assert not has_unregistered_inventory_handler(patch)
    assert not sibling_script_casts(patch)

    names = [name for name, _start, _end in _members(patch)]
    assert Counter(names) == Counter(set(names))
    merged = _merged(script_name)
    merged_names = [name for name, _start, _end in _members(merged)]
    for member_name in names:
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize(("script_name", "stages"), BOUND_FRAGMENT_STAGES.items())
def test_hubre_qf_matches_exact_live_vmad_fragment_set(
    script_name: str, stages: tuple[int, ...]
) -> None:
    fragments = {
        name
        for name, _start, _end in _members(_patch(script_name))
        if name.startswith("fragment_stage_")
    }
    assert fragments == {f"fragment_stage_{stage:04d}_item_00" for stage in stages}


def test_base_controller_is_quest_local_and_repeat_ready() -> None:
    patch = _patch(BASE)
    selection = _member_body(patch, "selectscenelocation")
    assert "Locations" in selection
    assert "SceneCenter.ForceRefTo(marker)" in selection
    assert "SetStage(candidate.StageToSet)" in selection
    assert "Game.GetFormFromFile" not in patch
    assert "SendStoryEvent" not in patch

    completion = _member_body(patch, "recordencounter")
    assert completion.count("player.SetValue(") == 1
    cleanup = _member_body(patch, "clearlocalselection")
    assert "SceneCenter.Clear()" in cleanup
    assert "ChosenLocation = None" in cleanup


def test_lost_and_found_selects_and_cleans_only_bound_local_items() -> None:
    patch = _patch(LOST)
    item_location = _member_body(patch, "selectitemlocation")
    assert "ItemLocs" in item_location
    assert "IsStageDone(candidate.MatchingAreaSceneStage)" in item_location
    missing = _member_body(patch, "selectmissingitem")
    assert "LostItems" in missing
    assert "PlaceAtMe(ChosenItem.ItemToFind" in missing
    assert "ChosenMissingItem.ForceRefTo(missingItemRef)" in missing
    cleanup = _member_body(patch, "clearlostandfoundselection")
    assert "missingItemRef.Delete()" in cleanup
    assert "ChosenMissingItem.Clear()" in cleanup

    for qf in (CHESS, HONOR, READING):
        patch = _patch(qf)
        assert "SetObjectiveDisplayed(20)" in patch
        assert "SetObjectiveDisplayed(30)" in patch
        assert "SetObjectiveCompleted(30)" in _member_body(
            patch, "fragment_stage_9000_item_00"
        )


def test_repair_family_uses_configured_choices_and_consumes_success_once() -> None:
    repair = _patch(REPAIR)
    assert "RobotLocs" in _member_body(repair, "selectrobotlocation")
    assert "RepairItems" in _member_body(repair, "selectrepairitem")
    assert "MoveActorToMarker(Robot, ChosenRobotLocation)" in _member_body(
        repair, "placerobot"
    )

    choices = _patch(REPAIR_MESSAGE)
    challenge = _member_body(choices, "challengepassed")
    assert "player.GetItemCount(challenge.Item_ToCheck)" in challenge
    assert "player.GetValue(challenge.SPECIAL_ToCheck)" in challenge
    assert "challenge.SPECIAL_Challenge_Global.GetValue()" in challenge
    consume = _member_body(choices, "consumerepairitems")
    assert consume.count("player.RemoveItem(") == 1
    activate = _member_body(choices, "onactivate")
    assert "owner.SetStage(challenge.StageToSet_OnPASS)" in activate
    assert "owner.SetStage(challenge.StageToSet_OnFAIL)" in activate

    for qf in (CONVERSION, DOG, MAKEOVER, FUSSFUNGLE, TOOTHACHE):
        patch = _patch(qf)
        assert "SetObjectiveDisplayed(20)" in patch
        assert "SetObjectiveDisplayed(30)" in patch


def test_photo_family_uses_explicit_fo4_camera_and_proximity_substitute() -> None:
    patch = _patch(PHOTO)
    selection = _member_body(patch, "selectphototarget")
    assert "Targets" in selection
    assert "Alias_PhotoTarget.ForceRefTo" in selection
    validator = _member_body(patch, "ontimer")
    assert "PlayerRef.GetItemCount(P01C_Bucket_BrokenCamera) > 0" in validator
    assert "PlayerRef.GetDistance(targetRef) <= 300.0" in validator
    assert "PlayerRef.AddItem(photographItem, 1, True)" in validator
    assert "SetStage(CompletionStage)" in validator
    assert "photo mode" not in patch.casefold()
    assert "photomode" not in patch.casefold()

    for qf in (CAMERA_SHY, HOTEL, IMPOSTOR, LETTERS, SURPRISES):
        patch = _patch(qf)
        assert "BeginLocalPhotoValidation" in patch
        assert "SetObjectiveDisplayed(30)" in patch


def test_settle_dispute_variants_restore_bound_actor_state() -> None:
    assert "JanitorClothes" in _patch(GARBAGE)
    lovers = _patch(LOVERS)
    assert "Outfit_A_Radiation" in lovers
    assert "Outfit_A_PA" in lovers
    assert "Outfit_A_Dirt" in lovers
    recruitment = _patch(RECRUITMENT)
    assert "recruit.SetValue(Recruited, 1.0)" in recruitment
    assert "MantaMask" in _patch(SHOWDOWN)
    assert "Outfit_Trooper" in _patch(WEAPON)


def test_hubre_completion_is_reward_neutral_and_stops_local_quests() -> None:
    combined = "\n".join(_patch(script_name) for script_name in PATCHED_SCRIPTS)
    assert "QuestReward" not in combined
    assert "RewardCaps" not in combined
    assert "SendStoryEvent" not in combined
    assert "Game.GetFormFromFile" not in combined

    for qf in QFS:
        terminal = _member_body(_patch(qf), "fragment_stage_9000_item_00")
        assert terminal.count("RecordEncounter(") == 1
        assert "AddItem(" not in terminal
        if qf != SURPRISES:
            assert "Controller().ScheduleLocalStop()" in terminal
            assert not re.search(r"(?m)^\s*Stop\(\)\s*$", terminal)
    surprise_terminal = _member_body(
        _patch(SURPRISES), "fragment_stage_9000_item_00"
    )
    assert "Controller().ScheduleLocalStop()" not in surprise_terminal
    assert "RegisterForPostCompleteLoad()" in surprise_terminal


def test_no_surprises_postcomplete_scene_reaches_9200_and_stops_once() -> None:
    patch = _patch(SURPRISES)
    stage_9000 = _member_body(patch, "fragment_stage_9000_item_00")
    stage_9100 = _member_body(patch, "fragment_stage_9100_item_00")
    begin = _member_body(patch, "beginpostcompletescene")
    scene_end = _member_body(patch, "scene.onend")
    load = _member_body(patch, "actor.onplayerloadgame")
    complete = _member_body(patch, "completepostcompleteencounter")
    shutdown = _member_body(patch, "onquestshutdown")

    assert "RecordEncounter(Alias_Player, NumEncounters)" in stage_9000
    assert "StartLocalScene(Ambient_PostComplete)" not in stage_9000
    assert "RegisterForPostCompleteLoad()" in stage_9000
    assert "BeginPostCompleteScene()" in stage_9100
    assert begin.index('RegisterForRemoteEvent(Ambient_PostComplete, "OnEnd")') < (
        begin.index("StartLocalScene(Ambient_PostComplete)")
    )
    assert "!Ambient_PostComplete.IsPlaying()" in begin
    assert "akSender == Ambient_PostComplete" in scene_end
    assert "CompletePostCompleteEncounter()" in scene_end
    assert "IsStageDone(9000)" in load
    assert "IsStageDone(9100)" in load
    assert "Ambient_PostComplete.IsPlaying()" in load
    assert "IsStageDone(9200)" in load
    assert complete.index("SetStage(9200)") < complete.index("Stop()")
    assert "IsStageDone(9200) && IsRunning()" in complete
    assert patch.count("\n\t\tStop()") == 1
    assert "UnregisterPostCompleteEvents()" in shutdown


def test_hubre_shared_stop_waits_for_rewards_and_reconciles_after_load() -> None:
    patch = _patch(BASE)
    timer_id = _member_body(patch, "localstoptimerid")
    schedule = _member_body(patch, "schedulelocalstop")
    timer = _member_body(patch, "ontimer")
    load = _member_body(patch, "actor.onplayerloadgame")
    shutdown = _member_body(patch, "onquestshutdown")
    photo_timer = _member_body(_patch(PHOTO), "ontimer")

    assert "Return 2140" in timer_id
    assert 'RegisterForRemoteEvent(player, "OnPlayerLoadGame")' in schedule
    assert "CancelTimer(LocalStopTimerID())" in schedule
    assert "StartTimer(2.0, LocalStopTimerID())" in schedule
    assert "aiTimerID == LocalStopTimerID()" in timer
    assert "IsRunning() && IsCompleted()" in timer
    assert timer.count("Stop()") == 1
    assert "IsRunning() && IsCompleted()" in load
    assert "ScheduleLocalStop()" in load
    assert "CancelTimer(LocalStopTimerID())" in shutdown
    assert 'UnregisterForRemoteEvent(player, "OnPlayerLoadGame")' in shutdown
    assert "If aiTimerID != 1" in photo_timer
    assert "Parent.OnTimer(aiTimerID)" in photo_timer
    for family in (LOST, REPAIR, PHOTO):
        family_shutdown = _member_body(_patch(family), "onquestshutdown")
        assert "Parent.OnQuestShutdown()" in family_shutdown


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_hubre_full_production_merge_native_compiles_for_fo4(
    script_name: str,
) -> None:
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
    assert result.ok, f"{script_name}: {diagnostics}"
    assert result.pex_bytes is not None
