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


def test_local_startup_and_fisher_setup_stages_are_reachable():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    startup = _member_body(patch, _stage_member(100))
    fisher_setup = _member_body(patch, _stage_member(500))
    assert startup.index("SetStage(1)") < startup.index("SetObjectiveDisplayed(100)")
    assert fisher_setup.index("SetStage(2)") < fisher_setup.index(
        "SetObjectiveDisplayed(500)"
    )


@pytest.mark.parametrize("stage", sorted(OBJECTIVE_RECEIVERS))
def test_every_objective_receiver_displays_its_bound_objective(stage: int):
    patch = _script_patch_source(QF_201)
    assert patch is not None
    assert f"SetObjectiveDisplayed({stage})" in _member_body(
        patch, _stage_member(stage)
    )


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


def test_only_server_respawn_checkpoint_is_an_explicit_documented_noop():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    assert ONLINE_NOOP_STAGES == {1510}
    body = _member_body(patch, _stage_member(1510))
    assert "FO76 respawn checkpoints are server/account-side" in body
    assert body.splitlines()[-2].strip() == "Return"


def test_fisher_branches_preserve_truth_lie_and_terminal_routes():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    truth = _member_body(patch, _stage_member(210))
    lie = _member_body(patch, _stage_member(211))
    gift = _member_body(patch, _stage_member(220))
    terminal = _member_body(patch, _stage_member(410))

    assert "SetValue(W05_MQR_201P_FisherLieValue, 0.0)" in truth
    assert "SetStage(500)" in lie
    assert "AddItem(SuperStimpak, 1, True)" in gift
    assert "SetStage(500)" not in gift
    assert "SetStage(700)" in terminal
    assert "SetStage(500)" not in terminal


def test_kogan_outcomes_converge_on_fisher_without_skipping_the_beacon():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    for stage in (615, 620):
        body = _member_body(patch, _stage_member(stage))
        assert "SetObjectiveCompleted(600)" in body
        assert "SetStage(700)" in body
        assert "SetStage(800)" not in body


def test_radio_route_uses_story_tune_and_distance_producers_in_order():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    stage_800 = _member_body(patch, _stage_member(800))
    stage_860 = _member_body(patch, _stage_member(860))

    assert "SetObjectiveDisplayed(850)" in stage_800
    assert "W05_MQR_201P_Track_RadioQuestStartKeyword.SendStoryEvent" in stage_800
    assert "SetStage(860)" not in stage_800
    assert "SetObjectiveCompleted(850)" in stage_860
    assert "SetStage(900)" not in stage_860


def test_scenes_and_triggers_own_the_two_wall_sequences():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    contracts = {
        1300: ("W05_MQR_201P_Weasel_004_GoToWall01.Start()", "SetStage(1400)"),
        1410: ("W05_MQR_201P_Weasel_006_BlowUpWall01.Start()", "SetStage(1420)"),
        1420: ("wallActivatorRef.Activate(weaselRef)", "SetStage(1600)"),
        1600: ("W05_MQR_201P_Weasel_007_BlowUpWall02.Start()", "SetStage(1620)"),
        1620: ("wallActivatorRef.Activate(weaselRef)", "SetStage(1700)"),
    }
    for stage, (required, forbidden) in contracts.items():
        body = _member_body(patch, _stage_member(stage))
        assert required in body
        assert forbidden not in body


def test_deathtrap_callbacks_comment_and_recover_pathing_without_skips():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    topics = {
        1430: "W05_MQR_201P_Weasel_Deathtrap01_Comment01",
        1440: "W05_MQR_201P_Weasel_Deathtrap02_Comment01",
        1500: "W05_MQR_201P_Weasel_Deathtrap03_Comment01",
        1520: "W05_MQR_201P_Weasel_Deathtrap03_Comment03A",
    }
    for stage, topic in topics.items():
        body = _member_body(patch, _stage_member(stage))
        assert f"weaselRef.Say({topic}" in body
        assert "SetStage(" not in body

    fallback = _member_body(patch, _stage_member(1530))
    assert "weaselRef.MoveTo(checkpointRef)" in fallback
    assert "W05_MQR_201P_Weasel_Deathtrap03_Comment03B" in fallback
    assert "SetStage(" not in fallback


def test_lou_and_meg_dialogue_callbacks_do_not_force_later_conversations():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    required_and_forbidden = {
        1700: ("W05_MQR_201P_Weasel_ApproachingLou", "SetStage(1705)"),
        1740: ("SetObjectiveFailed(1705)", "SetStage(1800)"),
        1810: ("SetValue(W05_MQR_LouPromiseValue, 1.0)", "SetStage(1900)"),
        1901: ("SetValue(W05_MQR_201P_ToldMegAboutLouValue, 1.0)", "SetStage(1910)"),
        1910: ("gailRef.EvaluatePackage()", "SetStage(9000)"),
    }
    for stage, (required, forbidden) in required_and_forbidden.items():
        body = _member_body(patch, _stage_member(stage))
        assert required in body
        assert forbidden not in body


def test_success_applies_local_rep_before_the_202_story_handoff():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    body = _member_body(patch, _stage_member(9000))
    rep = "playerRef.ModValue(Reputation_AV_Crater, Rep_Mod_Add_MQ.GetValue())"
    handoff = "W05_MQR_202P_QuestStart_Keyword.SendStoryEvent"

    assert rep in body
    assert "server/account-side" in body
    assert handoff in body
    assert body.index(rep) < body.index(handoff)


@pytest.mark.parametrize("stage", (9999, 10000))
def test_failure_and_shutdown_cleanup_only_a_running_radio_child(stage: int):
    patch = _script_patch_source(QF_201)
    assert patch is not None
    body = _member_body(patch, _stage_member(stage))

    assert "W05_MQR_201P_Track_RadioQuest.IsRunning()" in body
    assert "W05_MQR_201P_Track_RadioQuest.SetStage(1000)" in body
    assert "\n    Stop()" not in body


def test_intercom_delays_one_greeting_and_never_skips_to_1300():
    patch = _script_patch_source(INTERCOM)
    assert patch is not None
    assert set(_member_names(patch)) == {"ontriggerenter", "ontriggerleave", "ontimer"}
    enter = _member_body(patch, "ontriggerenter")
    timer = _member_body(patch, "ontimer")

    assert "akActionRef != playerRef" in enter
    assert "StartTimer(1.0, SayTimerID)" in enter
    assert "intercomRef.Say(" in timer
    assert "owningQuest.SetStage(PlayerTalkedToIntercomStage)" in timer
    assert timer.index("intercomRef.Say(") < timer.index(
        "owningQuest.SetStage(PlayerTalkedToIntercomStage)"
    )
    assert "SetStage(1300)" not in patch


def test_radio_child_stops_at_its_record_proven_cleanup_stage():
    patch = _script_patch_source(TRACK_RADIO)
    assert patch is not None
    assert [line.strip() for line in patch.splitlines() if line.strip()] == [
        "Function Fragment_Stage_1000_Item_00()",
        "Stop()",
        "EndFunction",
    ]


def test_explosive_breakers_require_every_bound_breaker_to_be_closed():
    patch = _script_patch_source(EXPLOSIVE_BREAKERS)
    assert patch is not None
    body = _member_body(patch, "onclose")
    for snippet in (
        "owningQuest == None || owningQuest.IsStageDone(1745)",
        "(Self as RefCollectionAlias).GetCount()",
        "breakerCount == 0",
        "(Self as RefCollectionAlias).GetAt(breakerIndex)",
        "breakerRef == None || breakerRef.GetOpenState() != 3",
        "owningQuest.SetStage(1745)",
    ):
        assert snippet in body


@pytest.mark.parametrize("script_name", COMPILE_CASES)
def test_tracked_zero_member_merge_is_exact_unique_and_idempotent(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    skeleton = TRACKED_SKELETONS[script_name]
    merged = _merged_tracked_source(script_name)

    assert _member_names(skeleton) == []
    assert Counter(_member_names(merged)) == Counter(_member_names(patch))
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", COMPILE_CASES)
def test_tracked_fixture_native_compiles_without_generated_sources(
    script_name: str, tmp_path: Path
):
    import_root = _minimal_import_root(tmp_path)
    result = compile_psc(
        _merged_tracked_source(script_name),
        imports=[str(import_root)],
        game="fo4",
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
