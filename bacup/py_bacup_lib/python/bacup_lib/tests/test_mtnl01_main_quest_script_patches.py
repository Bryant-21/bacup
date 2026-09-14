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

QUEST_FRAGMENT = "Fragments:Quests:QF_MTNL01_Raiders_00045A40"
SCRIPT_NAMES = (QUEST_FRAGMENT, "MTNL01QuestScript", "MTNL01PlayerScript")

BOUND_FRAGMENTS = {
    "fragment_stage_0100_item_00",
    "fragment_stage_0150_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0205_item_00",
    "fragment_stage_0210_item_00",
    "fragment_stage_0211_item_00",
    "fragment_stage_0220_item_00",
    "fragment_stage_0250_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0310_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0405_item_00",
    "fragment_stage_0406_item_00",
    "fragment_stage_0408_item_00",
    "fragment_stage_0410_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_0510_item_00",
    "fragment_stage_0511_item_00",
    "fragment_stage_0520_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0610_item_00",
    "fragment_stage_0611_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_0800_item_00",
    "fragment_stage_0801_item_00",
    "fragment_stage_0805_item_00",
    "fragment_stage_0810_item_00",
    "fragment_stage_0900_item_00",
    "fragment_stage_0950_item_00",
    "fragment_stage_1000_item_00",
    "fragment_stage_1050_item_00",
    "fragment_stage_1075_item_00",
    "fragment_stage_1100_item_00",
    "fragment_stage_1200_item_00",
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
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


def test_mtnl01_restores_every_source_bound_fragment_once() -> None:
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    patch_members = _member_names(patch)
    assert BOUND_FRAGMENTS <= set(patch_members)
    assert len(BOUND_FRAGMENTS) == 34
    for member in BOUND_FRAGMENTS:
        assert patch_members.count(member) == 1

    assert not any(
        line.strip().casefold().startswith("scriptname ")
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


def test_mtnl01_objectives_encounters_items_and_dialogue_cover_the_full_route() -> None:
    patch = _script_patch_source(QUEST_FRAGMENT)
    player_patch = _script_patch_source("MTNL01PlayerScript")
    assert patch is not None
    assert player_patch is not None

    assert "SetObjectiveDisplayed(100, True)" in _member_body(
        patch, "fragment_stage_0100_item_00"
    )
    assert "SetObjectiveCompleted(810, True)" in _member_body(
        patch, "fragment_stage_0900_item_00"
    )
    assert "SetObjectiveCompleted(1100, True)" in _member_body(
        patch, "fragment_stage_1200_item_00"
    )

    assert "StartLocalEncounterWave(0)" in _member_body(
        patch, "fragment_stage_0211_item_00"
    )
    assert "StartLocalEncounterWave(1)" in _member_body(
        patch, "fragment_stage_0511_item_00"
    )
    david_wave = _member_body(patch, "fragment_stage_0611_item_00")
    assert "StartLocalEncounterWave(2)" in david_wave
    assert "StartLocalEncounterWave(3)" in david_wave

    assert "Alias_BlackwaterBanditsKey" in _member_body(
        patch, "fragment_stage_0220_item_00"
    )
    assert "Alias_TrappersKeyContainer" in _member_body(
        patch, "fragment_stage_0310_item_00"
    )
    assert "Alias_AdminPassword" in _member_body(
        patch, "fragment_stage_0408_item_00"
    )
    assert "Alias_DiehardsKey" in _member_body(
        patch, "fragment_stage_0500_item_00"
    )
    assert "Alias_GourmandsKey" in _member_body(
        patch, "fragment_stage_0520_item_00"
    )
    assert "Alias_CutthroatsKey" in _member_body(
        patch, "fragment_stage_0610_item_00"
    )
    assert "MTNL01_RaiderCacheKeycard" in _member_body(
        patch, "fragment_stage_1000_item_00"
    )

    assert "MTNL01_Raiders_EBSTopic_LeavingRose" in patch
    assert "MTNL01_Raiders_EBSTopic_End" in patch
    assert "Alias_LaserGrids.DisableAll()" in _member_body(
        patch, "fragment_stage_1075_item_00"
    )

    added = _member_body(player_patch, "onitemadded")
    assert "MTNL01_TrappersNote" in added
    assert "MTNL01_MargieHolotape" in added
    assert "MTNL01_GourmandsNote" in added
    assert added.count("ForceRefTo(akItemReference)") == 3

    alias_init = _member_body(player_patch, "onaliasinit")
    assert alias_init.count("AddInventoryEventFilter(") == 3
    assert "MTNL01_TrappersNote" in alias_init
    assert "MTNL01_MargieHolotape" in alias_init
    assert "MTNL01_GourmandsNote" in alias_init
    assert "RemoveAllInventoryEventFilters()" in _member_body(
        player_patch, "onaliasshutdown"
    )


def _stage_950_actions(
    *,
    player: bool,
    quest: bool,
    start_keyword: bool,
    active_keyword: bool,
    completed: bool,
    stage_600_done: bool,
    running_before_send: bool,
    accepted_by_keyword: bool,
    running_after_send: bool,
    stage_300_done: bool,
) -> tuple[bool, bool]:
    if not player or not quest or not start_keyword or not active_keyword:
        return False, False
    if completed or stage_600_done:
        return False, False

    accepted = running_before_send or accepted_by_keyword
    sends_story_event = not running_before_send and not accepted
    sets_stage_300 = (
        not completed
        and not stage_600_done
        and running_after_send
        and not stage_300_done
    )
    return sends_story_event, sets_stage_300


@pytest.mark.parametrize(
    ("case", "kwargs", "expected"),
    (
        (
            "running below stage 300",
            {
                "running_before_send": True,
                "running_after_send": True,
            },
            (False, True),
        ),
        (
            "running at or after stage 300",
            {
                "running_before_send": True,
                "running_after_send": True,
                "stage_300_done": True,
            },
            (False, False),
        ),
        (
            "event starts the quest",
            {"running_after_send": True},
            (True, True),
        ),
        (
            "event reports success without a running target",
            {"running_after_send": False},
            (True, False),
        ),
        (
            "active keyword without a running target",
            {"accepted_by_keyword": True},
            (False, False),
        ),
        ("completed", {"completed": True}, (False, False)),
        (
            "completed while running",
            {"completed": True, "running_before_send": True, "running_after_send": True},
            (False, False),
        ),
        ("stage 600 complete", {"stage_600_done": True}, (False, False)),
        ("missing player", {"player": False}, (False, False)),
        ("missing quest", {"quest": False}, (False, False)),
        ("missing start keyword", {"start_keyword": False}, (False, False)),
        ("missing active keyword", {"active_keyword": False}, (False, False)),
    ),
)
def test_mtnl01_stage_950_safety_contract(
    case: str, kwargs: dict[str, bool], expected: tuple[bool, bool]
) -> None:
    defaults = {
        "player": True,
        "quest": True,
        "start_keyword": True,
        "active_keyword": True,
        "completed": False,
        "stage_600_done": False,
        "running_before_send": False,
        "accepted_by_keyword": False,
        "running_after_send": False,
        "stage_300_done": False,
    }
    assert _stage_950_actions(**(defaults | kwargs)) == expected, case


def test_mtnl01_stage_950_resumes_the_missing_link_only_after_a_safe_start() -> None:
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    stage_950 = _member_body(patch, "fragment_stage_0950_item_00")
    lines = [line.strip() for line in stage_950.splitlines() if line.strip()]
    assert lines[2] == (
        "If playerRef == None || MTN_MQ_Missing == None || "
        "MTN_MQ_Missing_Quest_Keyword == None || "
        "MTN_MQ_Missing_QuestActive_Keyword == None"
    )
    assert "Bool missingQuestTerminal = MTN_MQ_Missing.IsCompleted() || MTN_MQ_Missing.IsStageDone(600)" in lines
    assert "Bool missingQuestRunning = MTN_MQ_Missing.IsRunning()" in lines
    assert (
        "Bool missingQuestAccepted = missingQuestRunning || "
        "playerRef.HasKeyword(MTN_MQ_Missing_QuestActive_Keyword)"
    ) in lines

    send_index = lines.index(
        "MTN_MQ_Missing_Quest_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    assert lines[send_index - 1] == "If !missingQuestRunning && !missingQuestAccepted"
    assert "missingQuestAccepted = MTN_MQ_Missing_Quest_Keyword.SendStoryEventAndWait" not in stage_950
    reread_index = lines.index("missingQuestRunning = MTN_MQ_Missing.IsRunning()")
    assert reread_index > send_index
    assert lines[reread_index + 1] == (
        "missingQuestTerminal = MTN_MQ_Missing.IsCompleted() || "
        "MTN_MQ_Missing.IsStageDone(600)"
    )
    set_stage_index = lines.index("MTN_MQ_Missing.SetStage(300)")
    assert lines[set_stage_index - 1] == (
        "If !missingQuestTerminal && missingQuestRunning && "
        "!MTN_MQ_Missing.IsStageDone(300)"
    )
    assert ".Start()" not in stage_950

    stage_900 = _member_body(patch, "fragment_stage_0900_item_00")
    assert "MTNL01_Raiders_EBSTopic_End" in stage_900
    assert "SetStage(950)" in stage_900


def test_mtnl01_uses_quest_rewards_once_with_only_a_missing_pach_fallback() -> None:
    fragment_patch = _script_patch_source(QUEST_FRAGMENT)
    quest_patch = _script_patch_source("MTNL01QuestScript")
    assert fragment_patch is not None
    assert quest_patch is not None

    reward_stages = (303, 403, 503, 603, 703, 803, 903)
    for stage in reward_stages:
        assert f"SetStage({stage})" in fragment_patch

    quest_members = _member_names(quest_patch)
    assert "onstageset" not in quest_members
    assert "givesingleplayerreward" not in quest_members
    for reward_form in (
        "0x001EA88D",
        "0x001EA88E",
        "0x001EA88F",
        "0x001EA890",
        "0x001EA892",
        "0x001EA891",
        "0x0008212C",
    ):
        assert reward_form not in quest_patch

    final_reward = _member_body(quest_patch, "givesingleplayerfinalreward")
    assert "0x0051B4DB" in final_reward
    assert "if chassisReward != None" in final_reward
    assert "return" in final_reward
    for raider_piece in (
        "0x00140C54",
        "0x00140C57",
        "0x00140C52",
        "0x00140C53",
        "0x00140C55",
        "0x00140C56",
    ):
        assert final_reward.count(
            f"GiveSinglePlayerBaseGameReward({raider_piece})"
        ) == 1

    for duplicate_reward in (
        "0x001909D6",
        "0x004F478E",
        "0x003CA66B",
        "0x003CC4A1",
        "0x0000000F",
        "70",
    ):
        assert duplicate_reward not in final_reward

    completion = _member_body(fragment_patch, "fragment_stage_1200_item_00")
    assert "GiveSinglePlayerFinalReward()" in completion
    assert "CompleteQuest()" in completion
    assert "SetStage(1300)" in completion
    assert "Stop()" in completion

    shutdown = _member_body(quest_patch, "onquestshutdown")
    assert "CancelTimer(TrapWarningId)" in shutdown
    assert "RemovePerk(MTNL01_ExamineTrap_Perk)" in shutdown
    assert shutdown.count("RemoveQuestItem(playerRef") == 10


def test_mtnl01_preserves_the_existing_trap_repair() -> None:
    patch = _script_patch_source("MTNL01QuestScript")
    assert patch is not None

    assert "RegisterForRemoteEvent(keyRef, \"OnContainerChanged\")" in _member_body(
        patch, "onquestinit"
    )
    assert "StartTimer(warningLength, TrapWarningId)" in _member_body(
        patch, "triggertrap"
    )
    assert "explosionMarker.PlaceAtMe(ExplosionFatMan)" in _member_body(
        patch, "ontimer"
    )
    assert "akOwner.RemovePerk(MTNL01_ExamineTrap_Perk)" in _member_body(
        patch, "disarmtrap"
    )


@pytest.mark.parametrize("script_name", SCRIPT_NAMES)
def test_mtnl01_patches_merge_idempotently_and_full_sources_compile(
    script_name: str,
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_source(script_name)

    merged_members = _member_names(merged)
    for member in _member_names(patch):
        assert merged_members.count(member) == 1
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
