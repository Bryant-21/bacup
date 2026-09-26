from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


QF_201 = "Fragments:Quests:QF_W05_MQR_201P_0040D28D"
TRACK_RADIO = "Fragments:Quests:QF_W05_MQR_201P_Track_RadioQ_0040D28C"
INTERCOM = "W05_MQR_201P_IntercomTriggerScript"
EXPLOSIVE_BREAKERS = "W05_MQR_201P_ExplosiveBreakerScript"

COMPILE_CASES = (QF_201, TRACK_RADIO, INTERCOM, EXPLOSIVE_BREAKERS)
LIVE_BOUND_QF_STAGES = {
    1,
    2,
    3,
    4,
    100,
    200,
    210,
    211,
    220,
    300,
    400,
    410,
    500,
    600,
    610,
    615,
    620,
    700,
    800,
    860,
    900,
    1000,
    1100,
    1200,
    1300,
    1400,
    1410,
    1420,
    1430,
    1440,
    1500,
    1510,
    1520,
    1530,
    1600,
    1620,
    1700,
    1705,
    1740,
    1745,
    1800,
    1801,
    1810,
    1900,
    1901,
    1910,
    9000,
    9999,
    10000,
}
OBJECTIVE_RECEIVERS = {
    100,
    200,
    300,
    400,
    500,
    600,
    610,
    700,
    800,
    900,
    1000,
    1100,
    1200,
    1300,
    1400,
    1700,
    1705,
    1800,
    1900,
}
ONLINE_NOOP_STAGES = {1510}

TRACKED_SKELETONS = {
    QF_201: """Scriptname Fragments:Quests:QF_W05_MQR_201P_0040D28D Extends Quest

Topic Property W05_MQR_201P_Weasel_Deathtrap03_Comment01 Auto
ReferenceAlias Property Alias_Kogan Auto
ReferenceAlias Property Alias_LouNoteMarker Auto
ActorValue Property W05_MQR_201P_ToldMegAboutLouValue Auto
ActorValue Property W05_MQR_201P_CompletedMineValue Auto
ActorValue Property W05_MQR_GailAwayValue Auto
ReferenceAlias Property Alias_RespawnCheckPoint01 Auto
GlobalVariable Property Rep_Mod_Add_MQ Auto
ReferenceAlias Property Alias_Gail Auto
ActorValue Property Reputation_AV_Crater Auto
Keyword Property W05_MQR_201P_Track_RadioQuestStartKeyword Auto
ReferenceAlias Property Alias_LouNote Auto
Topic Property W05_MQR_201P_Weasel_ApproachingLou Auto
Scene Property W05_MQR_201P_Weasel_007_BlowUpWall02 Auto
Topic Property W05_MQR_201P_Weasel_Deathtrap02_Comment01 Auto
Topic Property W05_MQR_201P_Weasel_Deathtrap01_Comment01 Auto
ReferenceAlias Property Alias_WallActivator02 Auto
ReferenceAlias Property Alias_WallActivator01 Auto
Topic Property W05_MQR_201P_Weasel_Deathtrap03_Comment03B Auto
Topic Property W05_MQR_201P_Weasel_Deathtrap03_Comment03A Auto
Scene Property W05_MQR_201P_Weasel_006_BlowUpWall01 Auto
Scene Property W05_MQR_201P_Weasel_004_GoToWall01 Auto
Scene Property W05_MQR_201P_Weasel_000_StandAndFacePlayer Auto
Keyword Property W05_MQR_202P_QuestStart_Keyword Auto
ReferenceAlias Property Alias_Lou Auto
ReferenceAlias Property Alias_Weasel Auto
Quest Property W05_MQR_201P_Track_RadioQuest Auto
ActorValue Property W05_MQR_201P_FisherLieValue Auto
ActorValue Property W05_MQR_201P_PlayerFisherTerminalValue Auto
ActorValue Property W05_MQR_201P_FisherStimpakValue Auto
Potion Property SuperStimpak Auto
ActorValue Property W05_MQR_LouPromiseValue Auto
""",
    TRACK_RADIO: """Scriptname Fragments:Quests:QF_W05_MQR_201P_Track_RadioQ_0040D28C Extends Quest
""",
    INTERCOM: """Scriptname W05_MQR_201P_IntercomTriggerScript Extends ReferenceAlias

Int Property WeaselFollowingStage = 1100 Auto
Topic Property W05_MQR_201P_LouSaysTopic_IntercomGreeting Auto
ReferenceAlias Property LouIntercom Auto
ReferenceAlias Property Lou Auto
ReferenceAlias Property currentPlayer Auto
Int Property SayTimerID = 1 Auto
Int Property PlayerTalkedToIntercomStage = 1210 Auto
""",
    EXPLOSIVE_BREAKERS: """Scriptname W05_MQR_201P_ExplosiveBreakerScript Extends RefCollectionAlias
""",
}

MINIMAL_COMPILE_IMPORTS = {
    "Actor.psc": """Scriptname Actor Extends ObjectReference
Function EvaluatePackage() Native
Function SetValue(ActorValue akActorValue, Float afValue) Native
Function ModValue(ActorValue akActorValue, Float afAmount) Native
""",
    "ActorValue.psc": "Scriptname ActorValue Extends Form\n",
    "Alias.psc": """Scriptname Alias Extends Form
Quest Function GetOwningQuest() Native
""",
    "Form.psc": """Scriptname Form
Function StartTimer(Float afInterval, Int aiTimerID = 0) Native
Function CancelTimer(Int aiTimerID = 0) Native
""",
    "Game.psc": """Scriptname Game
Actor Function GetPlayer() Global Native
""",
    "GlobalVariable.psc": """Scriptname GlobalVariable Extends Form
Float Function GetValue() Native
""",
    "Keyword.psc": """Scriptname Keyword Extends Form
Function SendStoryEvent(Location akLocation = None, ObjectReference akRef1 = None, ObjectReference akRef2 = None) Native
""",
    "Location.psc": "Scriptname Location Extends Form\n",
    "ObjectReference.psc": """Scriptname ObjectReference Extends Form
Function EnableNoWait() Native
Function Activate(ObjectReference akActivator) Native
Function MoveTo(ObjectReference akTarget) Native
Function Say(Topic akTopic, Actor akActor = None, Bool abPlayersHead = False, ObjectReference akTarget = None) Native
Function AddItem(Form akItem, Int aiCount = 1, Bool abSilent = False) Native
Int Function GetOpenState() Native
""",
    "Potion.psc": "Scriptname Potion Extends Form\n",
    "Quest.psc": """Scriptname Quest Extends Form
Function SetObjectiveDisplayed(Int aiObjective) Native
Function SetObjectiveCompleted(Int aiObjective) Native
Function SetObjectiveFailed(Int aiObjective) Native
Bool Function IsObjectiveDisplayed(Int aiObjective) Native
Bool Function IsObjectiveCompleted(Int aiObjective) Native
Bool Function IsObjectiveFailed(Int aiObjective) Native
Bool Function IsStageDone(Int aiStage) Native
Bool Function SetStage(Int aiStage) Native
Int Function GetStage() Native
Function Stop() Native
""",
    "RefCollectionAlias.psc": """Scriptname RefCollectionAlias Extends ReferenceAlias
Int Function GetCount() Native
ObjectReference Function GetAt(Int aiIndex) Native
""",
    "ReferenceAlias.psc": """Scriptname ReferenceAlias Extends Alias
ObjectReference Function GetReference() Native
Actor Function GetActorReference() Native
""",
    "Scene.psc": """Scriptname Scene Extends Form
Bool Function IsPlaying() Native
Function Start() Native
""",
    "Topic.psc": "Scriptname Topic Extends Form\n",
    "W05_MQR_201P_QuestScript.psc": """Scriptname W05_MQR_201P_QuestScript Extends Quest
Function StartSignalTracking()
EndFunction
Function StopSignalTracking()
EndFunction
""",
}


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
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"} and name == member_name
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _stage_member(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


def _merged_tracked_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(TRACKED_SKELETONS[script_name], patch)


def _minimal_import_root(tmp_path: Path) -> Path:
    import_root = tmp_path / "papyrus_imports"
    import_root.mkdir()
    for file_name, source in MINIMAL_COMPILE_IMPORTS.items():
        (import_root / file_name).write_text(source, encoding="utf-8")
    return import_root


def test_qf_authors_every_live_vmad_member_exactly_once():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    expected = {_stage_member(stage) for stage in LIVE_BOUND_QF_STAGES}

    assert len(LIVE_BOUND_QF_STAGES) == 49
    assert set(_member_names(patch)) == expected
    assert Counter(_member_names(patch)) == Counter({name: 1 for name in expected})
    assert "; TODO" not in patch


def test_route_critical_members_are_not_hollow():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    for stage in LIVE_BOUND_QF_STAGES - ONLINE_NOOP_STAGES:
        body = _member_body(patch, _stage_member(stage))
        significant = [
            line.strip()
            for line in body.splitlines()[1:-1]
            if line.strip() and not line.lstrip().startswith(";")
        ]
        assert significant, stage
    noop = _member_body(patch, _stage_member(1510))
    assert noop.splitlines()[-2].strip() == "Return"


@pytest.mark.parametrize("stage", sorted(OBJECTIVE_RECEIVERS))
def test_every_objective_receiver_displays_its_bound_objective(stage: int):
    patch = _script_patch_source(QF_201)
    assert patch is not None
    assert f"SetObjectiveDisplayed({stage})" in _member_body(
        patch, _stage_member(stage)
    )


@pytest.mark.parametrize(
    ("script_name", "member", "required", "forbidden"),
    [
        (QF_201, _stage_member(211), "SetStage(500)", None),
        (QF_201, _stage_member(220), "AddItem(SuperStimpak, 1, True)", "SetStage(500)"),
        (QF_201, _stage_member(410), "SetStage(700)", "SetStage(500)"),
        (QF_201, _stage_member(615), "SetStage(700)", "SetStage(800)"),
        (QF_201, _stage_member(620), "SetStage(700)", "SetStage(800)"),
        (QF_201, _stage_member(800), "W05_MQR_201P_Track_RadioQuestStartKeyword.SendStoryEvent", "SetStage(860)"),
        (QF_201, _stage_member(860), "SetObjectiveCompleted(850)", "SetStage(900)"),
        (QF_201, _stage_member(1300), "W05_MQR_201P_Weasel_004_GoToWall01.Start()", "SetStage(1400)"),
        (QF_201, _stage_member(1420), "wallActivatorRef.Activate(weaselRef)", "SetStage(1600)"),
        (QF_201, _stage_member(1620), "wallActivatorRef.Activate(weaselRef)", "SetStage(1700)"),
        (QF_201, _stage_member(1500), "weaselRef.Say(W05_MQR_201P_Weasel_Deathtrap03_Comment01", "SetStage("),
        (QF_201, _stage_member(1530), "weaselRef.MoveTo(checkpointRef)", "SetStage("),
        (QF_201, _stage_member(1740), "SetObjectiveFailed(1705)", "SetStage(1800)"),
        (QF_201, _stage_member(1910), "gailRef.EvaluatePackage()", "SetStage(9000)"),
        (QF_201, _stage_member(9000), "W05_MQR_202P_QuestStart_Keyword.SendStoryEvent", None),
        (QF_201, _stage_member(9999), "W05_MQR_201P_Track_RadioQuest.SetStage(1000)", "\n    Stop()"),
        (TRACK_RADIO, "fragment_stage_1000_item_00", "Stop()", None),
        (INTERCOM, "ontimer", "owningQuest.SetStage(PlayerTalkedToIntercomStage)", "SetStage(1300)"),
        (EXPLOSIVE_BREAKERS, "onclose", "breakerRef == None || breakerRef.GetOpenState() != 3", None),
    ],
)
def test_stage_progression_contract(script_name, member, required, forbidden):
    patch = _script_patch_source(script_name)
    assert patch is not None
    body = _member_body(patch, member)
    assert required in body
    if forbidden is not None:
        assert forbidden not in body


@pytest.mark.parametrize("script_name", COMPILE_CASES)
def test_tracked_merge_is_idempotent_and_native_compiles(
    script_name: str, tmp_path: Path
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    skeleton = TRACKED_SKELETONS[script_name]
    merged = _merged_tracked_source(script_name)

    assert _member_names(skeleton) == []
    assert Counter(_member_names(merged)) == Counter(_member_names(patch))
    assert _merge_script_method_patches(merged, patch) == merged

    result = compile_psc(
        merged,
        imports=[str(_minimal_import_root(tmp_path))],
        game="fo4",
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
