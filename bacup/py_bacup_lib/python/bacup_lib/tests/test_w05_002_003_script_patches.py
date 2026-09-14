from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_SOURCES_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

TRACKED_MINIMAL_SKELETONS = {
    "W05_002P_IntroSceneTriggerScript": """\
Scriptname W05_002P_IntroSceneTriggerScript Extends ReferenceAlias

String DejaChannel = "W05_MQ_002P_Radical"
topic Property W05_002P_RoperWarning Auto mandatory
referencealias Property Loudspeaker Auto mandatory
quest Property W05_MQ_002P_Radical Auto mandatory
actorvalue Property W05_MQ_002P_Radical_PlayerKilledRoper Auto mandatory
actorvalue Property W05_MQ_002P_Radical_HeardRoperWarning Auto mandatory
""",
    "W05_002P_Radical_QuestScript": """\
Scriptname W05_002P_Radical_QuestScript Extends Quest conditional

Int iPlayerEnemyTimerID = 5
Int Property iPlayerEnemyTimerLength = 3 Auto
Int Property iRadGangersPlayerEnemyStage = 746 Auto
""",
}


PATCH_CASES = {
    "W05_002P_IntroSceneTriggerScript": (
        "Event OnTriggerEnter(ObjectReference akActionRef)",
        "Loudspeaker.GetReference().Say(W05_002P_RoperWarning)",
        "Game.GetPlayer().SetValue(W05_MQ_002P_Radical_HeardRoperWarning, 1.0)",
    ),
    "W05_002P_RadicalHostilityTrigger": (
        "Event OnTriggerEnter(ObjectReference akSenderRef, ObjectReference akActionRef)",
        "W05_MQ_002P_Radical_PlayerKilledRoper != None &&",
        "Game.GetPlayer().AddToFaction(W05_RadicalEnemyFaction)",
    ),
    "W05_003P_EnterAnyTriggerRefColl": (
        "Event OnTriggerEnter(ObjectReference akSenderRef, ObjectReference akActionRef)",
        "owningQuest.IsStageDone(ShutDownStage)",
        "owningQuest.SetStage(StageToSet)",
    ),
    "W05_003P_HiddenDoorTriggerScript": (
        "Event OnTriggerEnter(ObjectReference akActionRef)",
        "enteringActor.GetValue(W05_MQ_003P_Muscle_PlayerCanAccessDuncan) > 0.0",
        "hiddenDoor.SetOpen(True)",
    ),
}

MUSIC_OVERRIDE_TRIGGER = "W05_003P_MusicOverrideTriggerScript"

ZERO_MEMBER_CASES = (
    "W05_002P_DeathclawIsleTriggerScript",
)

RADICAL_QUEST_FRAGMENT = "Fragments:Quests:QF_W05_MQ_002P_Radical_0040F5BE"


def _members(source: str) -> list[tuple[str, str]]:
    return [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines())
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"} and name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _merged_production_source(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / f"{script_name.lower()}.pex"
    if pex_path.is_file():
        skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    else:
        skeleton = TRACKED_MINIMAL_SKELETONS.get(script_name)
        if skeleton is None:
            pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("script_name", "snippets"), PATCH_CASES.items())
def test_w05_002_003_patch_has_one_evidence_backed_trigger_handler(
    script_name: str, snippets: tuple[str, ...]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert _members(patch) == [("event", "ontriggerenter")]
    for snippet in snippets:
        assert snippet in patch


def test_intro_scene_trigger_uses_reference_alias_callback_signature():
    patch = _script_patch_source("W05_002P_IntroSceneTriggerScript")

    assert patch is not None
    assert patch.splitlines()[0].strip() == (
        "Event OnTriggerEnter(ObjectReference akActionRef)"
    )
    assert "akSenderRef" not in patch


def test_music_override_trigger_has_complete_enter_and_leave_surface():
    patch = _script_patch_source(MUSIC_OVERRIDE_TRIGGER)
    assert patch is not None
    assert _members(patch) == [
        ("event", "ontriggerenter"),
        ("event", "ontriggerleave"),
    ]
    assert "akSenderRef" not in patch
    assert "akActionRef != Game.GetPlayer()" in patch
    assert "controller.StartLocalCombatMusic()" in patch
    assert "controller.StopLocalCombatMusic()" in patch


def test_radical_controller_has_exact_guarded_stage_745_timer_pair():
    patch = _script_patch_source("W05_002P_Radical_QuestScript")

    assert patch is not None
    assert _members(patch) == [
        ("function", "getcampencountermarker"),
        ("function", "spawnencounteractor"),
        ("function", "spawnfirstencounter"),
        ("function", "spawnsecondencounter"),
        ("function", "trystartmusclequest"),
        ("event", "onquestinit"),
        ("event", "onstageset"),
        ("event", "ontimer"),
    ]
    assert "auiStageID == 745" in patch
    assert "StartTimer(iPlayerEnemyTimerLength, iPlayerEnemyTimerID)" in patch
    assert "aiTimerID == iPlayerEnemyTimerID" in patch
    assert "SetStage(iRadGangersPlayerEnemyStage)" in patch
    assert patch.count("!IsStageDone(iRadGangersPlayerEnemyStage)") == 2
    assert "If auiStageID == iFirstTimeSceneStage" in patch
    quest_init = _member_body(patch, "OnQuestInit")
    assert "GetCurrentStageID() == iFirstTimeSceneStage" in quest_init
    assert "!IsStageDone(103) && !IsStageDone(105)" in quest_init
    assert "StartTimer(0.5, 2103)" in quest_init
    assert patch.count("StartTimer(0.5, 2103)") == 3
    assert "playerRef.IsInScene()" in patch
    assert "ElseIf !IsStageDone(103) && !IsStageDone(105)" in patch

    try_start = _member_body(patch, "TryStartMuscleQuest")
    scene_guard = "If playerRef == None || playerRef.IsInScene()"
    story_send = "startKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    target_state = "muscleQuest.IsRunning() || muscleQuest.IsCompleted()"
    pre_send_state = f"If muscleQuest == None || {target_state}"
    post_send_state = f"If {target_state}"
    guard_start = try_start.index(scene_guard)
    guard_end = try_start.index("EndIf", guard_start)
    assert try_start.count(target_state) == 2
    assert try_start.index(pre_send_state) < try_start.index(story_send)
    assert guard_start < guard_end < try_start.index(story_send)
    guarded_path = try_start[guard_start:guard_end]
    assert "StartTimer(1.0, 8950)" in guarded_path
    assert "Return" in guarded_path
    assert try_start.count(story_send) == 1
    assert try_start.index(story_send) < try_start.index(post_send_state)
    assert f"= {story_send}" not in try_start
    assert f"&& {story_send}" not in try_start
    assert try_start.rstrip().endswith("StartTimer(1.0, 8950)\nEndFunction")
    assert "SetStage(9000)" not in try_start

    on_stage_set = _member_body(patch, "OnStageSet")
    on_timer = _member_body(patch, "OnTimer")
    assert "auiStageID == 8950" in on_stage_set
    assert on_stage_set.count("TryStartMuscleQuest()") == 1
    assert "aiTimerID == 8950" in on_timer
    assert on_timer.count("TryStartMuscleQuest()") == 1
    assert patch.count(story_send) == 1


def test_radical_controller_bridges_intro_checkpoint_to_plan_handoff():
    """Stages 110/125/150 lost every FO76 trigger, so obj 100 never closed."""
    patch = _script_patch_source("W05_002P_Radical_QuestScript")
    assert patch is not None

    on_stage_set = _member_body(patch, "OnStageSet")
    assert "auiStageID == 103 || auiStageID == 105" in on_stage_set
    for stage in (110, 125, 150):
        assert f"!IsStageDone({stage})" in on_stage_set
        assert f"SetStage({stage})" in on_stage_set

    # 110 must precede 150: 150 displays objective 110, which is unreadable
    # until 110 has put the plans in the player's inventory.
    assert on_stage_set.index("SetStage(110)") < on_stage_set.index("SetStage(150)")

    # The 2103 timer must still gate on the scene so this cannot fire mid-conversation.
    on_timer = _member_body(patch, "OnTimer")
    assert "playerRef.IsInScene()" in on_timer
    assert "SetStage(103)" in on_timer


def test_radical_fragment_stage_150_closes_speak_to_duchess_objective():
    """The bridge is only useful if 150 still owns the objective transition."""
    patch = _script_patch_source(RADICAL_QUEST_FRAGMENT)
    assert patch is not None

    stage_150 = _member_body(patch, "Fragment_Stage_0150_Item_00")
    assert "SetObjectiveCompleted(100)" in stage_150
    assert "SetObjectiveDisplayed(110)" in stage_150

    stage_110 = _member_body(patch, "Fragment_Stage_0110_Item_00")
    assert "Alias_SignPlans.GetReference()" in stage_110
    assert "playerRef.AddItem(" in stage_110


def test_radical_fragment_debug_and_hostility_members_are_executable():
    patch = _script_patch_source(RADICAL_QUEST_FRAGMENT)
    assert patch is not None

    stage_1 = _member_body(patch, "Fragment_Stage_0001_Item_00")
    assert "server-authored" in stage_1
    assert "Return" in stage_1

    stage_2 = _member_body(patch, "Fragment_Stage_0002_Item_00")
    assert "Stage2DebugMarker" in stage_2
    assert "playerRef.MoveTo(Stage2DebugMarker)" in stage_2

    stage_745 = _member_body(patch, "Fragment_Stage_0745_Item_00")
    assert "Utility.Wait(3.0)" in stage_745
    assert "!IsStageDone(746)" in stage_745
    assert "SetStage(746)" in stage_745


@pytest.mark.parametrize(
    "script_name",
    (*PATCH_CASES, MUSIC_OVERRIDE_TRIGGER, "W05_002P_Radical_QuestScript"),
)
def test_w05_002_003_merged_patch_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name", ZERO_MEMBER_CASES)
def test_w05_002_003_zero_member_scripts_have_no_hollow_patch(script_name: str):
    generated = (GENERATED_SOURCES_ROOT / f"{script_name}.psc").read_text(
        encoding="utf-8"
    )

    assert _script_patch_source(script_name) is None
    assert _members(generated) == []
