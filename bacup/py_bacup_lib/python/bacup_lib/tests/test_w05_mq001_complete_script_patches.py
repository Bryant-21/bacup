from __future__ import annotations

import re
from collections import Counter

from bacup_lib.workflows.unified import _script_patch_source


QF = "Fragments:Quests:QF_W05_MQ_001P_Wayward_00405E14"
LACEY_QF = "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_00405E15"
LACEY_ATTRACT_QF = "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_0053AF40"
MISC_POINTER_QF = "Fragments:Quests:QF_W05_MQ_001P_Wayward_MiscP_00594DFD"
BATTER_ALIAS = "W05_001P_BatterAliasScript"
QUEST_CONTROLLER = "W05_001P_Wayward_QuestScript"
BATTER_STAND_DOWN = "Fragments:Packages:PF_W05_MQ_001P_Wayward_Batte_0040BD22"

FULL_STAGE_MANIFEST = (
    10,
    200,
    300,
    400,
    600,
    103,
    105,
    301,
    302,
    310,
    445,
    450,
    455,
    460,
    470,
    491,
    510,
    515,
    516,
    520,
    522,
    530,
    550,
    598,
    610,
    620,
    660,
    680,
    705,
    710,
    809,
    820,
    500,
    599,
    805,
    807,
    900,
    905,
    9000,
    1000,
)
EXPLICIT_ONLINE_BOUNDARY_STAGES = (10, 705)


def _source(script_name: str) -> str:
    source = _script_patch_source(script_name)
    assert source is not None
    return source


def _stage_name(stage: int) -> str:
    return f"Fragment_Stage_{stage:04d}_Item_00"


def _function_body(source: str, name: str) -> str:
    match = re.search(
        rf"(?ims)^Function\s+{re.escape(name)}\([^\n]*\)\s*(.*?)^EndFunction\s*$",
        source,
    )
    assert match is not None, name
    return match.group(1)


def test_wayward_has_every_bound_member_and_a_continuous_single_player_route():
    qf = _source(QF)
    names = re.findall(r"(?im)^Function\s+(\w+)\(", qf)
    event_names = re.findall(r"(?im)^Event\s+(\w+)\(", qf)
    expected_names = [_stage_name(stage) for stage in FULL_STAGE_MANIFEST]
    fragment_names = [
        name for name in names if name.lower().startswith("fragment_stage_")
    ]
    assert Counter(name.lower() for name in fragment_names) == Counter(
        name.lower() for name in expected_names
    )
    assert len(fragment_names) == 40
    assert Counter(name.lower() for name in names) == Counter(
        [*(name.lower() for name in expected_names), "attemptradicalhandoff"]
    )
    assert Counter(name.lower() for name in event_names) == Counter(["ontimer"])

    for stage in FULL_STAGE_MANIFEST:
        body = _function_body(qf, _stage_name(stage))
        executable_lines = [
            line
            for line in body.splitlines()
            if line.strip() and not line.lstrip().startswith(";")
        ]
        assert executable_lines, stage

    for stage in EXPLICIT_ONLINE_BOUNDARY_STAGES:
        assert "Return" in _function_body(qf, _stage_name(stage))

    failed_diplomacy = _function_body(qf, _stage_name(522))
    assert "batterRef.StartCombat(playerRef)" in failed_diplomacy
    assert "playerRef.AddToFaction(W05_MQ_001P_BatterEnemyFaction)" in failed_diplomacy

    assert "SetObjectiveCompleted(100)" in _function_body(qf, _stage_name(105))
    assert "SetStage(200)" in _function_body(qf, _stage_name(105))
    assert "SetStage(300)" in _function_body(qf, _stage_name(301))
    assert "SetStage(300)" in _function_body(qf, _stage_name(302))

    confrontation = _function_body(qf, _stage_name(400))
    assert "SetObjectiveCompleted(300)" in confrontation
    assert "SetObjectiveDisplayed(400)" in confrontation
    assert "W05_MQ_001P_Wayward_400_Scene.Start()" in confrontation
    assert "SetStage(405)" in confrontation

    batter = _source(BATTER_ALIAS)
    assert "Event OnDeath(Actor akKiller)" in batter
    assert batter.index("owningQuest.SetStage(470)") < batter.index(
        "owningQuest.SetStage(599)"
    )
    assert "owningQuest.SetStage(599)" in batter

    batter_dead = _function_body(qf, _stage_name(599))
    assert "SetStage(598)" in batter_dead
    assert "SetStage(600)" in batter_dead
    attacker_done = _function_body(qf, _stage_name(600))
    assert "SetObjectiveCompleted(400)" in attacker_done
    assert "SetObjectiveDisplayed(600)" in attacker_done

    stand_down = _source(BATTER_STAND_DOWN)
    assert "GetOwningQuest().SetStage(600)" in stand_down

    for stage in (900, 905):
        completion = _function_body(qf, _stage_name(stage))
        assert completion.index("SetStage(9000)") < completion.index(
            "AttemptRadicalHandoff()"
        )
        assert "SendStoryEventAndWait" not in completion
        assert ".Start()" not in completion

    handoff = _function_body(qf, "AttemptRadicalHandoff")
    target_state = "accepted = radicalQuest.IsRunning() || radicalQuest.IsCompleted()"
    send = (
        "W05_MQ_002P_Radical_QuestStartKeyword."
        "SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    assert 'Game.GetFormFromFile(0x0040F5BE, "SeventySix.esm") as Quest' in handoff
    assert handoff.count(target_state) == 2
    assert (
        handoff.index(target_state)
        < handoff.index(send)
        < handoff.rindex(target_state)
    )
    assert handoff.count(send) == 1
    assert f"accepted = {send}" not in handoff
    assert "If !accepted && IsStageDone(9000)" in handoff
    assert "StartTimer(5.0, 9000)" in handoff
    assert ".Start()" not in handoff

    timer = re.search(
        r"(?ims)^Event\s+OnTimer\([^\n]*\)\s*(.*?)^EndEvent\s*$", qf
    )
    assert timer is not None
    assert "aiTimerID == 9000 && IsStageDone(9000)" in timer.group(1)
    assert timer.group(1).count("AttemptRadicalHandoff()") == 1
    assert "SendStoryEventAndWait" not in timer.group(1)

    final_completion = _function_body(qf, _stage_name(9000))
    assert "SetObjectiveCompleted(600)" in final_completion
    assert "CompleteQuest()" in final_completion
    assert qf.count("CompleteQuest()") == 1

    lacey = _source(LACEY_QF)
    assert "W05_MQ_001P_Wayward.SetStage(200)" in _function_body(
        lacey, _stage_name(100)
    )
    assert "SetLaceyIselaCheckpoint(1.0)" in _function_body(
        lacey, _stage_name(30)
    )
    assert "SetLaceyIselaCheckpoint(10.0)" in _function_body(
        lacey, _stage_name(100)
    )
    lacey_completion = _function_body(lacey, _stage_name(200))
    assert lacey_completion.index("SetLaceyIselaCheckpoint(10.0)") < (
        lacey_completion.index("Stop()")
    )

    attract = _source(LACEY_ATTRACT_QF)
    assert Counter(re.findall(r"(?im)^Function\s+(\w+)\(", attract)) == Counter(
        [
            _stage_name(10),
            "DispatchWaywardStartEvent",
            _stage_name(100),
            _stage_name(1000),
        ]
    )
    assert "DispatchWaywardStartEvent()" in _function_body(
        attract, _stage_name(10)
    )
    assert "Start()" in _function_body(attract, _stage_name(100))
    assert "Stop()" in _function_body(attract, _stage_name(1000))

    misc_pointer = _source(MISC_POINTER_QF)
    assert Counter(re.findall(r"(?im)^Function\s+(\w+)\(", misc_pointer)) == Counter(
        [_stage_name(100), _stage_name(9000)]
    )
    assert "PlayerStartedMiscPointer, 1.0" in _function_body(
        misc_pointer, _stage_name(100)
    )
    assert "CompleteQuest" in _function_body(misc_pointer, _stage_name(9000))

    controller = _source(QUEST_CONTROLLER)
    assert "CheckPlayerLocation(playerRef.GetCurrentLocation())" in controller
    assert "SetStage(102)" in controller
