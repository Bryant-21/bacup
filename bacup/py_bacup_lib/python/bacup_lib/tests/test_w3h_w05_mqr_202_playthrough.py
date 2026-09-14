from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


QF_202P = "Fragments:Quests:QF_W05_MQR_202P_0041C9E6"
ID_READER = "W05_MQR_202P_IDCardReaderScript"
PLAYER_ALIAS = "W05_MQR_202P_PlayerScript"
RARA_ITEM = "W05_MQR_202P_RaRaItemPickedUpScript"
VENT_MARKER = "W05_MQR_202P_VentMarkerScript"
BOSS_VENT_SCENE = "Fragments:Scenes:SF_W05_MQR_202P_RaRaVent_160_0056B778"

PLAYTHROUGH_SCRIPTS = (
    QF_202P,
    ID_READER,
    PLAYER_ALIAS,
    RARA_ITEM,
    VENT_MARKER,
    BOSS_VENT_SCENE,
)

LIVE_BOUND_QF_STAGES = {
    1,
    2,
    11,
    12,
    13,
    14,
    15,
    16,
    17,
    18,
    19,
    20,
    21,
    22,
    23,
    24,
    25,
    26,
    27,
    28,
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

INSTANCE_ONLY_NOOP_STAGES = {1, 2, *range(11, 29)}

OBJECTIVE_IDENTITY_STAGES = {
    100,
    200,
    300,
    310,
    400,
    500,
    600,
    620,
    700,
    800,
    900,
    920,
    940,
    970,
    1000,
    1100,
    1300,
    1400,
    1500,
    1510,
    1600,
    1610,
    1700,
    1800,
}

# QUST VMAD evidence for the route-critical stage producers. The default alias
# scripts are shared runtime dependencies; this test owns their MQR 202 consumers.
ALIAS_STAGE_PRODUCERS = {
    108: ("DefaultAliasOnTriggerEnter", 301),
    65: ("DefaultCollectionAliasOnDeath", 310),
    13: ("DefaultAliasOnContainerChangedTo", 930),
    53: ("DefaultAliasOnContainerChangedTo", 930),
    142: ("DefaultAliasOnDeath", 995),
    112: ("DefaultAliasOnContainerChangedTo", 1017),
    63: ("DefaultCollectionAliasOnDeath", 1215),
    64: ("DefaultCollectionAliasOnDeath", 1225),
    55: ("DefaultAliasOnTriggerEnter", 1510),
    21: ("DefaultCollectionAliasOnDeath", 1530),
    12: ("DefaultAliasOnContainerChangedTo", 1600),
    60: ("DefaultAliasOnDeath", 1650),
}

ENCOUNTER_WAVE_INTERFACE = """Scriptname DefaultQuestEncounterWaveScript Extends Quest

Function StartLocalEncounterWave(Int aiWaveIndex)
EndFunction
"""

BOSS_VENT_CONTROLLER_INTERFACE = """Scriptname W05_MQR_202P_QuestScript Extends Quest

Function StartBossVentCycle()
EndFunction

Function BeginBossPeek()
EndFunction

Function DropBossVentItem()
EndFunction

Function EndBossPeek()
EndFunction

Function FinishBossPeekCycle()
EndFunction
"""

SCENE_INSTANCE_INTERFACE = """Scriptname SceneInstance Extends ScriptObject hidden

Quest Function GetOwningQuest() native
"""

TRACKED_SKELETONS = {
    QF_202P: """Scriptname Fragments:Quests:QF_W05_MQR_202P_0041C9E6 Extends Quest hidden

referencealias Property Alias_currentPlayer Auto mandatory
keyword Property W05_MQR_203P_QuestStart_Keyword Auto mandatory
weapon Property PulseGrenade Auto mandatory
refcollectionalias Property Alias_InitialRobots Auto mandatory
referencealias Property Alias_RaRa Auto mandatory
scene Property W05_MQR_202P_RaRaVent_0310_ExitVent Auto mandatory
referencealias Property Alias_SectorAlphaDoor01 Auto mandatory
refcollectionalias Property Alias_RobotsDoor01 Auto mandatory
referencealias Property Alias_SectorAlphaDoor02 Auto mandatory
scene Property W05_MQR_202P_RaRaVent_0800_EnterAndExitVent Auto mandatory
scene Property W05_MQR_202P_RaRa_004C_SnackEnd Auto mandatory
referencealias Property Alias_PowerArmor Auto mandatory
referencealias Property Alias_PowerArmorHelmet Auto mandatory
scene Property W05_MQR_202P_PA_SectorAlphaKeycard Auto mandatory
referencealias Property Alias_SectorBravoEntranceDoor Auto mandatory
refcollectionalias Property Alias_Turrets01 Auto mandatory
refcollectionalias Property Alias_Turrets02 Auto mandatory
scene Property W05_MQR_202P_RaRaVent_1500_PeekSequence Auto mandatory
referencealias Property Alias_SectorCharlieRobotsEnableMarker Auto mandatory
scene Property W05_MQR_202P_PA_SectorCharlieRobots Auto mandatory
refcollectionalias Property Alias_RobotsSectorCharlie Auto mandatory
referencealias Property Alias_SectorCharlieDoor Auto mandatory
scene Property W05_MQR_202P_RaRaVent_1650_ExitVent Auto mandatory
""",
    ID_READER: """Scriptname W05_MQR_202P_IDCardReaderScript Extends ReferenceAlias
""",
    PLAYER_ALIAS: """Scriptname W05_MQR_202P_PlayerScript Extends ReferenceAlias

location Property LocToxicGraftonSteelUndergroundLocation Auto mandatory
Int Property RaRaExitVentStage = 310 Auto
""",
    RARA_ITEM: """Scriptname W05_MQR_202P_RaRaItemPickedUpScript Extends ReferenceAlias
""",
    VENT_MARKER: """Scriptname W05_MQR_202P_VentMarkerScript Extends ReferenceAlias

scene Property SceneToPlay Auto mandatory
""",
    BOSS_VENT_SCENE: """Scriptname Fragments:Scenes:SF_W05_MQR_202P_RaRaVent_160_0056B778 Extends SceneInstance hidden
""",
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


def _tracked_skeleton(script_name: str) -> str:
    return TRACKED_SKELETONS[script_name]


def _merged_tracked_source(script_name: str) -> str:
    return _merge_script_method_patches(
        _tracked_skeleton(script_name), _patch(script_name)
    )


def test_qf_patch_authors_every_live_bound_fragment_exactly_once():
    patched = {_fragment_member(stage) for stage in LIVE_BOUND_QF_STAGES}
    assert set(_member_names(_patch(QF_202P))) == patched
    assert len(_member_names(_patch(QF_202P))) == 67


def test_objective_fragments_preserve_exact_stage_objective_identity():
    qf = _patch(QF_202P)

    for stage in OBJECTIVE_IDENTITY_STAGES:
        body = _stage_body(qf, stage)
        assert body.count(f"SetObjectiveDisplayed({stage})") == 1


def test_all_live_fragments_are_nonempty_including_explicit_server_only_noops():
    qf = _patch(QF_202P)

    for stage in INSTANCE_ONLY_NOOP_STAGES:
        body = _stage_body(qf, stage)
        assert "FO76" in body
        assert body.splitlines()[-2].strip() == "Return"

    for stage in LIVE_BOUND_QF_STAGES - INSTANCE_ONLY_NOOP_STAGES:
        body = _stage_body(qf, stage)
        assert any(
            effect in body
            for effect in (
                "SetObjective",
                "SetStage(",
                ".Start(",
                ".Enable",
                ".EvaluatePackage(",
                ".SetOpen(",
                ".AddItem(",
                ".SendStoryEvent(",
                "StartLocalEncounterWave(",
                "Stop()",
            )
        )


def test_qf_route_restores_local_waves_doors_vents_turrets_and_handoff():
    qf = _patch(QF_202P)

    assert "Alias_InitialRobots.EnableAll()" in _stage_body(qf, 301)
    assert "SetStage(310)" in _stage_body(qf, 301)
    assert "W05_MQR_202P_RaRaVent_0310_ExitVent.Start()" in _stage_body(qf, 310)

    reader_response = _stage_body(qf, 550)
    assert "sectorAlphaDoor.Lock(False)" in reader_response
    assert "sectorAlphaDoor.SetOpen(True)" in reader_response
    assert "SetStage(600)" in reader_response

    assert "SetStage(630)" in _stage_body(qf, 620)
    assert "StartLocalEncounterWave(0)" in _stage_body(qf, 630)
    assert "sectorAlphaDoor.SetOpen(True)" in _stage_body(qf, 650)
    assert "securityRoomDoor.SetOpen(True)" in _stage_body(qf, 750)
    assert "W05_MQR_202P_RaRaVent_0800_EnterAndExitVent.Start()" in _stage_body(qf, 800)

    for branch_stage in (930, 940):
        assert "W05_MQR_202P_RaRa_004C_SnackEnd.Start()" in _stage_body(
            qf, branch_stage
        )

    armor = _stage_body(qf, 1017)
    assert "powerArmor.Enable()" in armor
    assert "powerArmorHelmet.Enable()" in armor
    assert "W05_MQR_202P_PA_SectorAlphaKeycard.Start()" in armor
    assert "sectorBravoDoor.SetOpen(True)" in _stage_body(qf, 1150)

    assert "Alias_Turrets01.EnableAll()" in _stage_body(qf, 1210)
    assert "SetStage(1215)" in _stage_body(qf, 1210)
    assert "Alias_Turrets02.EnableAll()" in _stage_body(qf, 1220)
    assert "SetStage(1225)" in _stage_body(qf, 1220)

    assert "W05_MQR_202P_RaRaVent_1500_PeekSequence.Start()" in _stage_body(qf, 1500)
    assert "SetStage(1520)" in _stage_body(qf, 1510)
    assert "SetStage(1520)" in _stage_body(qf, 1511)
    charlie_wave = _stage_body(qf, 1520)
    assert "robotsEnableMarker.Enable()" in charlie_wave
    assert "W05_MQR_202P_PA_SectorCharlieRobots.Start()" in charlie_wave
    assert "SetStage(1530)" in charlie_wave
    assert "sectorCharlieDoor.SetOpen(True)" in _stage_body(qf, 1530)

    assert "StartLocalEncounterWave(1)" in _stage_body(qf, 1600)
    assert "bossVentController.StartBossVentCycle()" in _stage_body(qf, 1600)
    assert "W05_MQR_202P_RaRaVent_1650_ExitVent.Start()" in _stage_body(qf, 1650)
    for result_stage in (1810, 1811):
        assert "SetStage(9000)" in _stage_body(qf, result_stage)
    assert "Stop()" in _stage_body(qf, 9999)


def test_bound_alias_producer_contract_reaches_every_route_critical_consumer():
    assert ALIAS_STAGE_PRODUCERS == {
        108: ("DefaultAliasOnTriggerEnter", 301),
        65: ("DefaultCollectionAliasOnDeath", 310),
        13: ("DefaultAliasOnContainerChangedTo", 930),
        53: ("DefaultAliasOnContainerChangedTo", 930),
        142: ("DefaultAliasOnDeath", 995),
        112: ("DefaultAliasOnContainerChangedTo", 1017),
        63: ("DefaultCollectionAliasOnDeath", 1215),
        64: ("DefaultCollectionAliasOnDeath", 1225),
        55: ("DefaultAliasOnTriggerEnter", 1510),
        21: ("DefaultCollectionAliasOnDeath", 1530),
        12: ("DefaultAliasOnContainerChangedTo", 1600),
        60: ("DefaultAliasOnDeath", 1650),
    }

    qf_members = set(_member_names(_patch(QF_202P)))
    for stage in (301, 310, 930, 1017, 1510, 1530, 1600, 1650):
        assert _fragment_member(stage) in qf_members


def test_dropped_item_stage_only_displays_the_dynamic_alias_objective():
    item_stage = _stage_body(_patch(QF_202P), 1610)

    assert item_stage.count("SetObjectiveDisplayed(1610)") == 1
    assert ".AddItem(" not in item_stage
    assert "PulseGrenade" not in item_stage
    assert "SetObjectiveCompleted" not in item_stage


def test_boss_vent_scene_dispatches_each_live_phase_to_the_controller():
    scene = _patch(BOSS_VENT_SCENE)

    assert _member_names(scene) == [
        "fragment_phase_01_begin",
        "fragment_phase_02_begin",
        "fragment_phase_08_begin",
        "fragment_phase_09_end",
    ]
    callbacks = {
        "fragment_phase_01_begin": "BeginBossPeek()",
        "fragment_phase_02_begin": "DropBossVentItem()",
        "fragment_phase_08_begin": "EndBossPeek()",
        "fragment_phase_09_end": "FinishBossPeekCycle()",
    }
    for phase, callback in callbacks.items():
        body = _member_body(scene, phase)
        assert "GetOwningQuest() as W05_MQR_202P_QuestScript" in body
        assert f"bossVentController.{callback}" in body


def test_completion_uses_the_bound_203p_story_manager_handoff():
    qf = _patch(QF_202P)

    completion = _stage_body(qf, 9000)
    assert "Alias_currentPlayer.GetReference()" in completion
    assert (
        "W05_MQR_203P_QuestStart_Keyword.SendStoryEvent(None, playerRef, playerRef)"
        in completion
    )
    assert ".Start()" not in completion


def test_alias_helpers_are_bounded_to_their_proven_local_receivers():
    reader = _patch(ID_READER)
    player = _patch(PLAYER_ALIAS)
    item = _patch(RARA_ITEM)
    vent = _patch(VENT_MARKER)

    assert reader.index("akActionRef != Game.GetPlayer()") < reader.index(
        "owningQuest.SetStage(550)"
    )
    assert player.index(
        "akNewLoc != LocToxicGraftonSteelUndergroundLocation"
    ) < player.index("owningQuest.SetStage(300)")
    assert player.index("owningQuest.IsStageDone(200)") < player.index(
        "owningQuest.SetStage(300)"
    )
    assert item.index("akNewContainer != Game.GetPlayer()") < item.index(
        "owningQuest.SetObjectiveCompleted(1610)"
    )
    assert vent.index("SceneToPlay != None") < vent.index("SceneToPlay.Start()")


def test_todo_markers_match_bounded_route_scope():
    assert _patch(QF_202P).splitlines().count("; TODO") == 0
    for script_name in (ID_READER, PLAYER_ALIAS, RARA_ITEM, VENT_MARKER):
        assert _patch(script_name).splitlines().count("; TODO") == 0


@pytest.mark.parametrize("script_name", PLAYTHROUGH_SCRIPTS)
def test_tracked_fixture_merge_is_exact_unique_and_idempotent(script_name: str):
    patch = _patch(script_name)
    merged = _merged_tracked_source(script_name)
    assert Counter(_member_names(merged)) == Counter(_member_names(patch))
    for member_name in _member_names(patch):
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", PLAYTHROUGH_SCRIPTS)
def test_full_tracked_fixture_merge_native_compiles_for_fo4(
    script_name: str, tmp_path: Path
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    support_source = tmp_path / "DefaultQuestEncounterWaveScript.psc"
    support_source.write_text(ENCOUNTER_WAVE_INTERFACE, encoding="utf-8")
    controller_source = tmp_path / "W05_MQR_202P_QuestScript.psc"
    controller_source.write_text(BOSS_VENT_CONTROLLER_INTERFACE, encoding="utf-8")
    scene_instance_source = tmp_path / "SceneInstance.psc"
    scene_instance_source.write_text(SCENE_INSTANCE_INTERFACE, encoding="utf-8")

    result = compile_psc(
        _merged_tracked_source(script_name),
        imports=[str(tmp_path), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
