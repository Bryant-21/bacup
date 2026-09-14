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
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Fragments:Quests:QF_CB04_Mayor_002A93F5"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
DEPLOYED_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"

CB04_PATCH_MEMBERS = {
    "CB_MasterScript": {
        "onquestinit",
        "actor.onplayerloadgame",
        "ondistancelessthan",
        "registerformayordistance",
    },
    "CB04_KeycardReaderAliasScript": {"onactivate"},
    "CB04_QuestScript": {
        "addclue",
        "givevirustoplayer",
        "spawnrobotwave",
        "cleanuprobotwaves",
        "startuploaddefense",
        "finishuploaddefense",
        "grantcompletionreward",
        "ontimer",
    },
    SCRIPT_NAME: {
        "fragment_stage_0010_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_0800_item_00",
        "fragment_stage_0810_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_9999_item_00",
    },
    "Fragments:Scenes:SF_CB04_Mayor_RoofC_004ECF09": {"fragment_phase_01_end"},
    "Fragments:Terminals:TERM_CB04_MayorTerminal_Impo_00044F89": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_CB04_SaboteurTerminal_H_0033CE9D": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_CB04_TowerTerminal_002AAB37": {"fragment_terminal_01"},
}


def test_cb04_mayor_completion_patch_merges_once_and_compiles():
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "Scriptname" not in patch
    assert "Property" not in patch
    assert patch.count("Function Fragment_Stage_9999_Item_00()") == 1
    assert "Actor playerRef = Game.GetPlayer()" in patch
    assert "Alias_Player.GetActorReference()" not in patch
    assert "playerRef.AddToFaction(PlayerFriendWatogaRobotFaction)" in patch

    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    assert members.count("fragment_stage_9999_item_00") == 1
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize(
    ("script_name", "expected_members"), CB04_PATCH_MEMBERS.items()
)
def test_all_cb04_mayor_patches_merge_once_and_compile_exact_production_skeleton(
    script_name: str, expected_members: set[str]
):
    pex_path = DEPLOYED_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), pex_path
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname" not in patch
    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected_members:
        assert members.count(member) == 1
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


def test_cb04_mayor_patch_restores_single_player_progression_contract():
    quest_patch = _script_patch_source(SCRIPT_NAME)
    quest_script_patch = _script_patch_source("CB04_QuestScript")

    assert quest_patch is not None
    assert quest_script_patch is not None
    assert "Alias_Player.ForceRefTo(playerRef)" in quest_patch
    assert 'RegisterForRemoteEvent(workTerminal, "OnMenuItemRun")' in quest_patch
    assert "mayorQuest.GiveVirusToPlayer()" in quest_patch
    assert "mayorQuest.StartUploadDefense()" in quest_patch
    assert "playerRef.AddToFaction(PlayerFriendWatogaRobotFaction)" in quest_patch
    assert "playerRef.StopCombatAlarm()" in quest_patch

    assert "FoundClues.Find(aiClue)" in quest_script_patch
    assert "FoundClues.Length >= CluesNeeded" in quest_script_patch
    assert quest_script_patch.count("SpawnRobotWave(") == 6
    assert "StartTimer(300.0, 199)" in quest_script_patch
    assert "SetStage(StageQuestCompleteSetActorValue)" in quest_script_patch
    assert "playerRef.AddItem(CB04_Reward_CombinationSafeKey, 1, False)" in (
        quest_script_patch
    )


def test_cb04_master_starts_mayor_for_a_day_from_its_bound_distance_marker():
    master_patch = _script_patch_source("CB_MasterScript")
    assert master_patch is not None

    assert "RegisterForMayorDistance()" in master_patch
    assert 'RegisterForRemoteEvent(player, "OnPlayerLoadGame")' in master_patch
    assert (
        "RegisterForDistanceLessThanEvent(player, CB04_StartOnDistanceLessThanMarkerRef, CB04_RadioDistance)"
        in master_patch
    )
    distance_index = master_patch.index("Event OnDistanceLessThan(")
    start_index = master_patch.index("CB04_Mayor.Start()", distance_index)
    stage_index = master_patch.index("CB04_Mayor.SetStage(10)", start_index)
    assert distance_index < start_index < stage_index
    assert "!CB04_Mayor.IsRunning() && !CB04_Mayor.IsCompleted()" in master_patch
