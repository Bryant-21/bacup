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
PATCH_CASES = {
    "OverseerPersonal_PlayerConnectScript": {"onquestinit"},
    "MQ_Overseer_PlayerConnectScript": {"onquestinit"},
    "DefaultAliasOnDistanceLessThan": {
        "onaliasinit",
        "quest.onstageset",
        "registerdistanceevent",
        "ondistancelessthan",
    },
    "DefaultQuestEnterInstancedLocScript": {
        "onquestinit",
        "actor.onlocationchange",
        "checkplayerlocation",
    },
    "Fragments:Quests:QF_Storm_MQ01_Breadcrumb_OnC_0072A2A7": {
        "fragment_stage_0100_item_00"
    },
    "Fragments:Quests:QF_GHL00_Quest_OnConnect_0078DB39": {
        "fragment_stage_0100_item_00"
    },
    "Fragments:Quests:QF_BURN_SQ01_OnConnect_007F79A9": {
        "fragment_stage_0100_item_00"
    },
    "Fragments:Quests:QF_XPD_Hub_Responders_OnConn_0064D323": {
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_9000_item_00",
        "startrespondersquest",
    },
    "Fragments:Quests:QF_BS01_MQ00_Breadcrumb_OnCo_005EAD3B": {
        "fragment_stage_0100_item_00"
    },
    "Fragments:Quests:QF_BS02_MQ01_Penance_OnConne_00606C1C": {
        "fragment_stage_0100_item_00"
    },
    "Fragments:Quests:QF_W05_MQ_101P_OnConnect_003FBBB4": {
        "fragment_stage_0010_item_00"
    },
}


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_player_connect_patch_is_member_only_and_complete(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None

    lines = patch.splitlines()
    assert set(_member_names(patch)) == PATCH_CASES[script_name]
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in lines
    )
    assert not any(" property " in f" {line.strip().lower()} " for line in lines)


@pytest.mark.parametrize(
    ("script_name", "keyword_name"),
    (
        (
            "OverseerPersonal_PlayerConnectScript",
            "OverseerPersonal_QuestStartKeyword",
        ),
        ("MQ_Overseer_PlayerConnectScript", "MQ_Overseer_QuestStartKeyword"),
    ),
)
def test_root_player_connect_quests_send_bound_story_event_then_stop(
    script_name: str, keyword_name: str
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    init = _member_body(patch, "onquestinit")

    send = f"{keyword_name}.SendStoryEventAndWait()"
    assert init.count(send) == 1
    assert init.index(f"{keyword_name} != None") < init.index(send)
    assert init.index(send) < init.index("Stop()")


@pytest.mark.parametrize(
    ("script_name", "target_name", "keyword_name"),
    (
        (
            "Fragments:Quests:QF_Storm_MQ01_Breadcrumb_OnC_0072A2A7",
            "Storm_MQ01_Breadcrumb",
            "Storm_MQ01_Breadcrumb_QuestStartKeyword",
        ),
        (
            "Fragments:Quests:QF_BURN_SQ01_OnConnect_007F79A9",
            "BURN_SQ01",
            "BURN_SQ01_QuestStartKeyword",
        ),
        (
            "Fragments:Quests:QF_BS01_MQ00_Breadcrumb_OnCo_005EAD3B",
            "BS01_MQ00_Breadcrumb",
            "BS01_MQ00_Breadcrumb_QuestStartKeyword",
        ),
        (
            "Fragments:Quests:QF_BS02_MQ01_Penance_OnConne_00606C1C",
            "BS02_MQ01_Penance",
            "BS02_MQ01_Penance_StartKeyword",
        ),
    ),
)
def test_guarded_stage_connectors_start_inactive_target_then_stop(
    script_name: str, target_name: str, keyword_name: str
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    stage = _member_body(patch, "fragment_stage_0100_item_00")

    send = f"{keyword_name}.SendStoryEventAndWait(None, playerRef, playerRef)"
    assert f"!{target_name}.IsRunning()" in stage
    assert f"!{target_name}.IsCompleted()" in stage
    assert stage.index(f"!{target_name}.IsRunning()") < stage.index(send)
    assert stage.index(send) < stage.index("Stop()")
    assert ".Start()" not in stage


def test_w05_new_arrivals_connector_sends_bound_event_then_stops():
    patch = _script_patch_source(
        "Fragments:Quests:QF_W05_MQ_101P_OnConnect_003FBBB4"
    )
    assert patch is not None
    stage = _member_body(patch, "fragment_stage_0010_item_00")

    send = (
        "W05_MQ_101P_QuestStartKeyword."
        "SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    assert stage.index("Alias_currentPlayer.GetReference()") < stage.index(send)
    assert stage.index(send) < stage.index("Stop()")
    assert ".Start()" not in stage


def test_ghl_connector_sends_bound_event_with_player_then_stops():
    patch = _script_patch_source(
        "Fragments:Quests:QF_GHL00_Quest_OnConnect_0078DB39"
    )
    assert patch is not None
    stage = _member_body(patch, "fragment_stage_0100_item_00")

    send = "GHL00_Quest_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    assert stage.index("Alias_Player.GetReference()") < stage.index(send)
    assert stage.index(send) < stage.index("Stop()")


def test_responders_connector_uses_both_arrival_paths_and_completes_once():
    patch = _script_patch_source(
        "Fragments:Quests:QF_XPD_Hub_Responders_OnConn_0064D323"
    )
    assert patch is not None

    for stage_name in (
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
    ):
        assert "StartRespondersQuest()" in _member_body(patch, stage_name)

    start = _member_body(patch, "startrespondersquest")
    assert "!Quest_Reborn.IsRunning()" in start
    assert "!Quest_Reborn.IsCompleted()" in start
    assert "!playerRef.HasKeyword(Responders_Active_Keyword)" in start
    send = "Responders_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    assert start.index(send) < start.index("SetStage(9000)")
    assert _member_body(patch, "fragment_stage_9000_item_00").count("Stop()") == 1


def test_distance_helper_honors_prerequisite_and_sets_bound_stage_once():
    patch = _script_patch_source("DefaultAliasOnDistanceLessThan")
    assert patch is not None

    register = _member_body(patch, "registerdistanceevent")
    assert "StageToRegister < 0 || OwningQuest.IsStageDone(StageToRegister)" in register
    assert "!OwningQuest.IsStageDone(StageToSet)" in register
    assert "RegisterForDistanceLessThanEvent(Self, TargetAlias, fTargetDistance)" in register

    event = _member_body(patch, "ondistancelessthan")
    assert event.index("OwningQuest.IsStageDone(StageToSet)") < event.index(
        "OwningQuest.SetStage(StageToSet)"
    )


def test_instanced_location_helper_uses_single_player_location_substitute():
    patch = _script_patch_source("DefaultQuestEnterInstancedLocScript")
    assert patch is not None

    init = _member_body(patch, "onquestinit")
    assert 'RegisterForRemoteEvent(playerRef, "OnLocationChange")' in init
    assert "CheckPlayerLocation(playerRef.GetCurrentLocation())" in init

    check = _member_body(patch, "checkplayerlocation")
    assert "stageData.TargetLocationAlias.GetLocation()" in check
    assert "stageData.PrereqStage < 0 || IsStageDone(stageData.PrereqStage)" in check
    assert "stageData.TurnOffStage < 0 || GetStage() < stageData.TurnOffStage" in check
    assert check.index("targetLocation == playerLocation") < check.index(
        "SetStage(stageData.StageToSet)"
    )
    assert "InstancedRefAlias" not in check


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_player_connect_production_merge_is_unique_and_idempotent(
    script_name: str,
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    merged_names = _member_names(merged)

    for member_name in PATCH_CASES[script_name]:
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_player_connect_full_production_merge_native_compiles_for_fo4(
    script_name: str,
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
