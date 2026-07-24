from __future__ import annotations

from collections import Counter
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
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)

QF_202P = "Fragments:Quests:QF_W05_MQR_202P_0041C9E6"
ID_READER = "W05_MQR_202P_IDCardReaderScript"
PLAYER_ALIAS = "W05_MQR_202P_PlayerScript"
RARA_ITEM = "W05_MQR_202P_RaRaItemPickedUpScript"
VENT_MARKER = "W05_MQR_202P_VentMarkerScript"

PLAYTHROUGH_SCRIPTS = (QF_202P, ID_READER, PLAYER_ALIAS, RARA_ITEM, VENT_MARKER)

BOUND_QF_STAGES = {
    1,
    2,
    *range(11, 29),
    100,
    200,
    300,
    301,
    310,
    400,
    500,
    550,
    600,
    605,
    610,
    620,
    630,
    650,
    700,
    750,
    800,
    900,
    910,
    920,
    930,
    940,
    970,
    1000,
    1010,
    1017,
    1020,
    1100,
    1150,
    1210,
    1220,
    1300,
    1400,
    1500,
    1510,
    1511,
    1520,
    1530,
    1600,
    1610,
    1650,
    1700,
    1800,
    1810,
    1811,
    9000,
    9999,
}


def _fragment_member(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _stage_body(source: str, stage: int) -> str:
    return _member_body(source, _fragment_member(stage))


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    return _merge_script_method_patches(
        _production_skeleton(script_name), _patch(script_name)
    )


def test_qf_members_are_all_live_vmad_bound_callables():
    expected = {_fragment_member(stage) for stage in BOUND_QF_STAGES}
    assert set(_member_names(_patch(QF_202P))) <= expected


def test_instance_entry_and_rara_scene_handoffs_preserve_authored_scenes():
    qf = _patch(QF_202P)

    stage_310 = _stage_body(qf, 310)
    assert "Alias_RaRa.GetActorReference()" in stage_310
    assert "W05_MQR_202P_RaRaVent_0310_ExitVent.Start()" in stage_310

    assert "raRaRef.EvaluatePackage()" in _stage_body(qf, 610)

    stage_800 = _stage_body(qf, 800)
    assert "W05_MQR_202P_RaRaVent_0800_EnterAndExitVent.Start()" in stage_800

    for stage in (970, 1010, 1300):
        assert "raRaRef.EvaluatePackage()" in _stage_body(qf, stage)

    assert "W05_MQR_202P_RaRaVent_1500_PeekSequence.Start()" in _stage_body(
        qf, 1500
    )

    peek_substitute = _stage_body(qf, 1510)
    assert peek_substitute.index("!IsStageDone(1520)") < peek_substitute.index(
        "SetStage(1520)"
    )

    security_robots = _stage_body(qf, 1520)
    assert "Alias_SectorCharlieRobotsEnableMarker.GetReference()" in security_robots
    assert security_robots.index("robotsEnableMarker != None") < security_robots.index(
        "robotsEnableMarker.Enable()"
    )
    assert "W05_MQR_202P_PA_SectorCharlieRobots.Start()" in security_robots
    assert (
        "robotsEnableMarker == None || Alias_RobotsSectorCharlie == None || "
        "Alias_RobotsSectorCharlie.GetCount() == 0"
    ) in security_robots
    assert security_robots.index("!IsStageDone(1530)") < security_robots.index(
        "SetStage(1530)"
    )

    assert "W05_MQR_202P_RaRaVent_1650_ExitVent.Start()" in _stage_body(
        qf, 1650
    )


def test_local_door_and_missing_encounter_substitutes_are_guarded():
    qf = _patch(QF_202P)

    reader = _stage_body(qf, 550)
    assert reader.index("!IsStageDone(600)") < reader.index("SetStage(600)")

    first_robots = _stage_body(qf, 620)
    assert "Alias_RobotsDoor01 == None || Alias_RobotsDoor01.GetCount() == 0" in first_robots
    assert first_robots.index("!IsStageDone(630)") < first_robots.index(
        "SetStage(630)"
    )

    alpha_door = _stage_body(qf, 650)
    assert "Alias_SectorAlphaDoor01.GetReference()" in alpha_door
    assert alpha_door.index("sectorAlphaDoor != None") < alpha_door.index(
        "sectorAlphaDoor.SetOpen(True)"
    )

    security_door = _stage_body(qf, 750)
    ordered_door_actions = (
        "Alias_SectorAlphaDoor02.GetReference()",
        "securityRoomDoor != None",
        "securityRoomDoor.Lock(False)",
        "securityRoomDoor.SetOpen(True)",
    )
    positions = [security_door.index(action) for action in ordered_door_actions]
    assert positions == sorted(positions)
    assert "SetStage(800)" not in security_door

    bravo_door = _stage_body(qf, 1150)
    assert "Alias_SectorBravoEntranceDoor.GetReference()" in bravo_door
    assert bravo_door.index("sectorBravoDoor != None") < bravo_door.index(
        "sectorBravoDoor.SetOpen(True)"
    )

    charlie_door = _stage_body(qf, 1530)
    assert "Alias_SectorCharlieDoor.GetReference()" in charlie_door
    assert charlie_door.index("sectorCharlieDoor != None") < charlie_door.index(
        "sectorCharlieDoor.SetOpen(True)"
    )

    boss = _stage_body(qf, 1600)
    assert "bossRef == None || bossRef.IsDisabled()" in boss
    assert boss.index("!IsStageDone(1650)") < boss.index("SetStage(1650)")


def test_dialogue_outcomes_preserve_scenes_and_reach_203p_story_manager_handoff():
    qf = _patch(QF_202P)

    for stage in (930, 940):
        body = _stage_body(qf, stage)
        assert "W05_MQR_202P_RaRa_004C_SnackEnd.Start()" in body
        assert "SetStage(970)" not in body

    qf_members = set(_member_names(qf))
    assert _fragment_member(1810) not in qf_members
    assert _fragment_member(1811) not in qf_members

    completion = _stage_body(qf, 9000)
    assert "Alias_currentPlayer.GetReference()" in completion
    assert (
        "W05_MQR_203P_QuestStart_Keyword.SendStoryEvent(None, playerRef, playerRef)"
        in completion
    )
    assert ".Start()" not in completion

    route_bodies = "\n".join(
        _stage_body(qf, stage)
        for stage in (550, 620, 750, 930, 940, 1530, 1600, 9000)
    )
    for forbidden in (
        "defaultquestencounterwavescript",
        "EncounterWaves",
        "Rep_Mod_",
        "Reputation_AV_",
        ".AddItem(",
        ".RemoveItem(",
        ".ModValue(",
    ):
        assert forbidden not in route_bodies


def test_alias_helpers_supply_their_local_stage_and_scene_events():
    reader = _patch(ID_READER)
    player = _patch(PLAYER_ALIAS)
    item = _patch(RARA_ITEM)
    vent = _patch(VENT_MARKER)

    assert reader.index("akActionRef != Game.GetPlayer()") < reader.index(
        "owningQuest.SetStage(550)"
    )
    assert player.index("akNewLoc != LocToxicGraftonSteelUndergroundLocation") < player.index(
        "owningQuest.SetStage(310)"
    )
    assert player.index("owningQuest.IsStageDone(200)") < player.index(
        "owningQuest.SetStage(310)"
    )
    assert item.index("akNewContainer != Game.GetPlayer()") < item.index(
        "owningQuest.SetObjectiveCompleted(1610)"
    )
    assert vent.index("SceneToPlay != None") < vent.index("SceneToPlay.Start()")


def test_todo_markers_match_bounded_route_scope():
    assert _patch(QF_202P).splitlines().count("; TODO") == 1
    for script_name in (ID_READER, PLAYER_ALIAS, RARA_ITEM, VENT_MARKER):
        assert _patch(script_name).splitlines().count("; TODO") == 0


@pytest.mark.parametrize("script_name", PLAYTHROUGH_SCRIPTS)
def test_production_merge_is_exact_unique_and_idempotent(script_name: str):
    patch = _patch(script_name)
    merged = _merged_production_source(script_name)
    assert Counter(_member_names(merged)) == Counter(_member_names(patch))
    for member_name in _member_names(patch):
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", PLAYTHROUGH_SCRIPTS)
def test_full_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    assert GENERATED_SOURCE_ROOT.is_dir(), "generated source root unavailable"

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(base_source), str(GENERATED_SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
