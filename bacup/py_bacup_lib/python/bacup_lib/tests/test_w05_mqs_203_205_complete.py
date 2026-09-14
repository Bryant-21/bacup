from __future__ import annotations

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
)


MQS_203 = "Fragments:Quests:QF_W05_MQS_203P_0040571C"
MQS_205 = "Fragments:Quests:QF_W05_MQS_205P_0041CB6D"

MQS_203_STAGES = (
    10, 100, 200, 300, 301, 310, 400, 500, 600, 700, 710, 720, 730,
    800, 801, 900, 905, 910, 920, 921, 930, 931, 940, 941, 950, 1000,
    1001, 1002, 1003, 1004, 1005, 1100, 1200, 1300, 1400, 1500, 1501,
    1510, 1520, 1530, 1600, 1700, 1800, 2000, 2100, 9000, 9999, 10000,
)
MQS_205_STAGES = (
    20, 10, 30, 40, 50, 100, 200, 250, 300, 350, 400, 450, 500, 700,
    800, 900, 950, 1000, 1050, 1100, 1300, 1400, 1500, 1600, 1800,
    1900, 1950, 2000, 2100, 2200, 2300, 9000, 10000,
)


def _members(source: str) -> dict[str, str]:
    lines = source.splitlines()
    return {
        name: "\n".join(lines[start : end + 1])
        for kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if kind == "function"
    }


def _member(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


def test_foundation_quest_fragments_cover_every_live_bound_member_once():
    cases = ((MQS_203, MQS_203_STAGES), (MQS_205, MQS_205_STAGES))
    for script_name, stages in cases:
        source = _script_patch_source(script_name)
        assert source is not None
        members = _members(source)
        assert tuple(members) == tuple(_member(stage) for stage in stages)
        assert len(members) == len(stages)
        assert "TODO" not in source
        assert all(len(body.splitlines()) > 2 for body in members.values())


def test_duty_calls_has_local_brain_prep_inventory_and_wave_progression():
    source = _script_patch_source(MQS_203)
    assert source is not None
    members = _members(source)

    learned = members[_member(600)]
    assert "SetObjectiveDisplayed(70)" in learned
    assert "SetValue(W05_MQS_203P_CanInteractBrokenRobo, 1.0)" in learned

    assembled = members[_member(1300)]
    assert assembled.index("assembledBrain = True") < assembled.index(
        "RemoveItem(W05_MQS_203P_RobobrainDome"
    )
    assert assembled.index("RemoveItem(W05_MQS_203P_RobobrainDome") < assembled.index(
        "SetObjectiveDisplayed(110)"
    )
    assert "SetStage(1400)" not in assembled

    making_tools = members[_member(1400)]
    assert "SetObjectiveCompleted(110)" in making_tools
    assert "SetObjectiveDisplayed(120)" in making_tools
    tools_finished = members[_member(1500)]
    assert "SetObjectiveCompleted(120)" in tools_finished
    assert "SetObjectiveDisplayed(130)" in tools_finished

    restored_tools = members[_member(1501)]
    for choice, stage in (("ChoseDias", 1510), ("ChoseGreg", 1520), ("ChoseGina", 1530)):
        assert f"W05_MQS_203P_{choice}" in restored_tools
        assert f"SetStage({stage})" in restored_tools

    picked_up = members[_member(1600)]
    assert "SetObjectiveCompleted(130)" in picked_up
    for av in ("HasToolsVolatile", "HasToolsStandard", "HasToolsClever"):
        assert f"SetValue(W05_MQS_203P_{av}, 1.0)" in picked_up
    assert "W05_MQS_203P_016_PickedUpToolsScene.Start()" in picked_up

    for stage, wave_index in ((1001, 0), (2000, 1), (2100, 2)):
        wave = members[_member(stage)]
        # Stock PapyrusCompiler rejects a direct sibling-script cast, so the
        # reach-across goes through the shared Quest base.
        assert "as DefaultQuestEncounterWaveScript" in wave
        assert "Self" in wave
        assert f"StartLocalEncounterWave({wave_index})" in wave


def test_duty_calls_brain_controls_are_player_guarded_and_stage_bounded():
    brain_prep = _script_patch_source("W05_MQS_203P_BrainPrepScript")
    placement = _script_patch_source("W05_MQS_203P_BrainPrepPlacementScript")
    broken_robot = _script_patch_source("W05_MQS_203P_BrokenRoboScript")
    assert brain_prep is not None
    assert placement is not None
    assert broken_robot is not None
    assert "akActionRef != player" in brain_prep
    assert "selectedButton == CorrectButton" in brain_prep
    assert "TryToSetStage()" in brain_prep
    assert "akActionRef != player" in placement
    assert placement.count("RemoveItem(W05_MQS_203P_BrainJar_") == 3
    assert "akActionRef != player" in broken_robot
    assert broken_robot.count("GetOwningQuest().SetStage(StageToSet)") == 1


def test_duty_calls_heist_route_waits_for_scene_and_combat_producers():
    source = _script_patch_source(MQS_205)
    assert source is not None
    members = _members(source)

    breach = members[_member(800)]
    assert "Alias_DirtEnableMarker.GetReference()" in breach
    assert "Alias_RobotStagingDoor.GetReference()" in breach
    assert "stagingDoor.SetOpen(True)" in breach

    robots_dead = members[_member(1300)]
    assert "W05_MQS_205P_011_RobotsDeadScene.Start()" in robots_dead
    assert "SetStage(1400)" not in robots_dead

    grid_off = members[_member(1400)]
    assert grid_off.count(".Disable()") == 5
    assert "SetValue(W05_MQS_205P_LaserGridState, 1.0)" in grid_off
    assert "SetStage(1500)" not in grid_off

    through_grid = members[_member(1000)]
    assert "W05_MQS_205P_008_LaserGridScene.Start()" not in through_grid
    assert "SetStage(1100)" not in members[_member(1050)]
    robots_attack = members[_member(1100)]
    assert "as DefaultQuestEncounterWaveScript" in robots_attack
    assert "Self" in robots_attack
    assert "StartLocalEncounterWave(0)" in robots_attack

    assert "pennyRef.EvaluatePackage()" in members[_member(1600)]
    assert "W05_MQS_205P_014_LaserTurretScene.Start()" in members[_member(1800)]
    assert "W05_MQS_205P_LastTurrets.Start()" in members[_member(1950)]
    assert "SetObjectiveDisplayed(140)" in members[_member(2000)]
    assert "SetObjectiveDisplayed(150)" in members[_member(2100)]
    assert "Stop()" not in members[_member(10000)]
