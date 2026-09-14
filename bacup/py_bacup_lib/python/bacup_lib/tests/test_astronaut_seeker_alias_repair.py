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
FIXTURE = Path(__file__).parent / "fixtures" / "fo76" / "astronaut_seeker_source_live.json"

PATCH_MEMBERS = {
    "W05_COMP_AstronautSeekerAliasScript": {
        "onaliasinit",
        "ontimer",
        "onaliasshutdown",
        "assignlocalseeker",
        "armrefillcheck",
    },
    "Fragments:Quests:QF_COMP_Quest_Intro_Astronau_005A0675": {
        "fragment_stage_0010_item_00",
        "fragment_stage_9000_item_00",
    },
    "Fragments:Quests:QF_COMP_Astronaut_Initial_0054EB40": {
        "fragment_stage_1190_item_00",
        "fragment_stage_1195_item_00",
    },
}


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_MEMBERS.items())
def test_astronaut_seeker_chain_merges_once_and_compiles(
    script_name: str, expected_members: set[str]
) -> None:
    skeleton = (
        SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    ).read_text(encoding="utf-8")
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


def test_astronaut_seeker_uses_timer_interval_and_living_wave_member() -> None:
    patch = _script_patch_source("W05_COMP_AstronautSeekerAliasScript")

    assert patch is not None
    assert "CancelTimer(CheckTimerID)" in patch
    assert "StartTimer(RefillCheck as Float, CheckTimerID)" in patch
    assert "If aiTimerID == CheckTimerID" in patch
    assert "currentSeeker != None && !currentSeeker.IsDead()" in patch
    assert "If SpawnEnemies == None" in patch
    assert "enemyIndex < SpawnEnemies.GetCount()" in patch
    assert "!enemy.IsDead()" in patch
    assert "ForceRefTo(enemy)" in patch
    assert "enemy.EvaluatePackage()" in patch
    assert patch.index("ForceRefTo(enemy)") < patch.index("enemy.EvaluatePackage()")
    assert "owner == None || !owner.IsRunning()" in patch
    assert "SpawnEnemies.GetCount() < RefillCheck" not in patch


def test_astronaut_intro_starts_and_stops_exact_local_wave_route() -> None:
    crash_patch = _script_patch_source(
        "Fragments:Quests:QF_COMP_Quest_Intro_Astronau_005A0675"
    )
    intro_patch = _script_patch_source(
        "Fragments:Quests:QF_COMP_Astronaut_Initial_0054EB40"
    )

    assert crash_patch is not None
    assert intro_patch is not None
    assert "materializer.PrepareEligibleWaves()" in crash_patch
    assert "StartLocalEncounterWave(0)" in crash_patch
    assert crash_patch.index("materializer.PrepareEligibleWaves()") < crash_patch.index(
        "StartLocalEncounterWave(0)"
    )
    assert "Function Fragment_Stage_9000_Item_00()" in crash_patch
    assert "Stop()" in crash_patch
    assert (
        "COMP_Keyword_QuestStart_Astronaut_Intro_SpawnQuest.SendStoryEventAndWait()"
        in intro_patch
    )
    assert "COMP_Quest_Intro_Astronaut_CrashSpawnQuest.SetStage(9000)" in intro_patch


def test_astronaut_source_live_carrier_evidence_is_frozen() -> None:
    evidence = json.loads(FIXTURE.read_text(encoding="utf-8"))

    assert evidence["main_quest"] == {
        "form_id": "0054EB40",
        "editor_id": "COMP_Quest_Intro_Full_Astronaut",
        "start_stage": 1190,
        "shutdown_stage": 1195,
        "start_keyword": "005A0681",
        "story_manager_node": "005A06A8",
        "started_quest": "005A0675",
    }
    assert evidence["quest"] == {
        "form_id": "005A0675",
        "editor_id": "COMP_Quest_Intro_Astronaut_CrashSpawnQuest",
        "source_event_type": "CoOp",
        "live_event_type": None,
        "stages": [10, 9000],
        "spawn_center_alias": 0,
        "wave_collection_alias": 2,
        "seeker_alias": 4,
        "seeker_package": "005A24B0",
    }
    assert evidence["wave"]["live_record_supported"] is False
    assert evidence["wave"]["actors"] == [
        {"form_id": "00075335", "spawn_slot": 0},
        {"form_id": "00075335", "spawn_slot": 0},
        {"form_id": "0014AE58", "spawn_slot": 0},
    ]
    assert evidence["seeker_package"]["source_and_live_target_marker"] == "005A24B1"
    assert evidence["seeker_client_pex"]["members"] == []
    assert evidence["seeker_client_pex"]["spawn_enemies_alias"] == 2
    assert evidence["seeker_client_pex"]["check_timer_id"] == 1
    assert evidence["seeker_client_pex"]["refill_check"] == 15
