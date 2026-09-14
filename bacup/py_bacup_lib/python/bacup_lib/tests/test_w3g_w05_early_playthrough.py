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

QF_000P = "Fragments:Quests:QF_W05_MQ_000P_005698E4"
QF_001P = "Fragments:Quests:QF_W05_MQ_001P_Wayward_00405E14"
QF_001P_LACEY = "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_00405E15"
QF_001P_ATTRACT = "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_0053AF40"
QF_001P_MISC = "Fragments:Quests:QF_W05_MQ_001P_Wayward_MiscP_00594DFD"
QF_002P = "Fragments:Quests:QF_W05_MQ_002P_Radical_0040F5BE"
QF_003P = "Fragments:Quests:QF_W05_MQ_003P_Muscle_0041A39D"
QF_003P_RADIO = "Fragments:Quests:QF_W05_MQ_003P_Radio_0041A325"
QF_003P_DUNCAN = "Fragments:Quests:QF_W05_MQ_003P_Muscle_Duncan_005537E0"
QF_004P = "Fragments:Quests:QF_W05_MQ_004P_Crane_0041C976"

ASSIGNED_SCRIPTS = (
    QF_000P,
    QF_001P,
    QF_001P_LACEY,
    QF_001P_ATTRACT,
    QF_001P_MISC,
    QF_002P,
    QF_003P,
    QF_003P_RADIO,
    QF_003P_DUNCAN,
    QF_004P,
)


def _member_name(stage: int) -> str:
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
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"} and name == member_name
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(script_name), patch)


CRITICAL_EDGES: dict[str, dict[int, tuple[str, ...]]] = {
    QF_000P: {
        2100: ("IsStageDone(2200)", "SetStage(2300)"),
        2200: ("SetValue(pW05_MQ00_CodeAV, 1.0)", "SetStage(2300)"),
        2300: ("SetValue(pW05_MQ00_Completed, 1.0)", "SetStage(9000)"),
    },
    QF_001P: {
        400: (
            "SetObjectiveCompleted(300)",
            "W05_MQ_001P_Wayward_400_Scene.Start()",
            "SetStage(405)",
        ),
        500: ("W05_MQ_001P_Wayward_500_Scene.Start()",),
        599: ("SetStage(598)", "SetStage(600)"),
        805: ("Alias_Duchess.GetActorReference()", "EvaluatePackage()"),
        807: ("SetStage(809)", "EvaluatePackage()"),
        900: (
            "SetStage(9000)",
            "AttemptRadicalHandoff()",
        ),
        905: (
            "SetStage(9000)",
            "AttemptRadicalHandoff()",
        ),
        1000: ("W05_MQ_003P_Muscle_QuestStartKeyword.SendStoryEvent()",),
    },
    QF_002P: {
        100: ("SetObjectiveDisplayed(100)",),
        450: (
            "SetValue(W05_MQ_002P_Radical_PlayerConnectedRadioStation, 1.0)",
            "SetObjectiveCompleted(400)",
            "W05_MQ_002P_Radical_0450_RadioSignConnected.Start()",
            "SetStage(475)",
        ),
        1550: ("SetStage(2000)",),
        8950: ("SetStage(9000)",),
    },
    QF_003P: {
        100: ("W05_MQ_003P_Muscle_0100_StartScene.Start()", "SetStage(150)"),
        400: ("W05_MQ_003P_Muscle_0400_SolAttactScene.Start()",),
        410: (
            "solRef.RemoveFromFaction(CaptiveFaction)",
            "solRef.AddToFaction(W05_CrimeTheWayward)",
        ),
        415: ("solRef.ResetHealthAndLimbs()",),
        450: (
            "solRef.ChangeAnimArchetype(AnimArchetypeDepressed)",
            "solRef.EvaluatePackage()",
        ),
        600: ("W05_MQ_003P_Muscle_0600_PollyAttractScene.Start()",),
        1050: ("W05_MQ_003P_Muscle_1010b_PollyEquipped.Start()",),
        1150: ("W05_MQ_003P_Muscle_1150_PollyScene.Start()",),
        1325: ("SetObjectiveCompleted(1300)", "SetObjectiveDisplayed(1315)"),
        1500: (
            "SetStage(9000)",
            "W05_MQ_004P_Crane_QuestStartKeyword.SendStoryEvent()",
        ),
    },
    QF_004P: {
        1000: ("cacheDoor.Unlock()", "cacheDoor.SetOpen(True)"),
        1150: (
            "roperRef && !roperRef.IsDead()",
            "roperRef.Enable()",
            "roperRef.EvaluatePackage()",
            "SetStage(1300)",
        ),
        1200: ("W05_MQ_004P_Crane_1200_RoperScene.Start()",),
        1250: ("SetStage(1300)",),
        1265: ("playerRef.AddItem(Headwear_Radicals, 1, False)",),
    },
}


@pytest.mark.parametrize("script_name", CRITICAL_EDGES)
def test_every_critical_receiving_edge_has_one_executable_fragment(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    names = Counter(_member_names(patch))

    for stage, required_snippets in CRITICAL_EDGES[script_name].items():
        member_name = _member_name(stage)
        assert names[member_name] == 1
        body = _member_body(patch, member_name)
        for snippet in required_snippets:
            assert snippet in body


def test_source_named_receiving_stages_preserve_guarded_routes():
    radical = _script_patch_source(QF_002P)
    muscle = _script_patch_source(QF_003P)
    crane = _script_patch_source(QF_004P)
    assert radical is not None
    assert muscle is not None
    assert crane is not None

    radical_450 = _member_body(radical, _member_name(450))
    assert "W05_MQ_002P_Radical_0450_RadioSignConnected.Start()" in radical_450
    assert "If !IsStageDone(475)" in radical_450
    assert "SetStage(475)" in radical_450
    for stale_bypass in ("SetStage(709)", "SetStage(736)", "SetStage(1000)"):
        assert stale_bypass not in radical_450

    muscle_710 = _member_body(muscle, _member_name(710))
    assert "encounterController.StartLocalEncounterWave(0)" in muscle_710
    assert "ElseIf !IsStageDone(715)" in muscle_710
    assert "SetStage(715)" in muscle_710

    muscle_900 = _member_body(muscle, _member_name(900))
    assert "pollyMarker.Disable()" in muscle_900
    assert "solMarker.Disable()" in muscle_900
    assert "playerRef.SetValue(W05_MQ_003P_Muscle_GauleyMineComplete, 1.0)" in muscle_900
    assert "If !IsStageDone(1000)" in muscle_900
    assert "SetStage(1000)" in muscle_900

    crane_820 = _member_body(crane, _member_name(820))
    assert "SetObjectiveDisplayed(800)" in crane_820
    assert "cacheDoor.Unlock()" in crane_820
    assert "SetStage(1100)" not in _member_body(crane, _member_name(1000))

    bypass_bodies = "\n".join(
        (
            radical_450,
            muscle_710,
            muscle_900,
            crane_820,
            _member_body(crane, _member_name(1000)),
        )
    )
    for excluded_surface in (
        "EWS",
        "SpawnEncounter",
        "Community",
        "Bounty",
        "Reputation",
        "AddItem(",
        "RemoveItem(",
    ):
        assert excluded_surface not in bypass_bodies


def test_existing_early_handoff_and_local_repairs_remain_present():
    expected = {
        QF_001P_LACEY: {
            15: ("DispatchWaywardStartEvent()",),
            30: (
                "SetLaceyIselaCheckpoint(1.0)",
                "W05_MQ_001P_Wayward_LaceyIselaScene_010.Stop()",
                "W05_MQ_001P_Wayward_LaceyIselaScene_020.Stop()",
            ),
            100: (
                "W05_MQ_001P_Wayward.SetStage(200)",
                "SetLaceyIselaCheckpoint(10.0)",
            ),
            200: ("SetLaceyIselaCheckpoint(10.0)", "Stop()"),
        },
        QF_001P_ATTRACT: {
            100: ("W05_MQ_001P_Wayward_LaceyIselaAtrractScene_0100_Intro.Start()",),
            1000: ("W05_MQ_001P_Wayward_LaceyIselaAtrractScene_0100_Intro.Stop()",),
        },
        QF_001P_MISC: {
            100: ("W05_MQ_001P_Wayward_PlayerStartedMiscPointer, 1.0",),
        },
        QF_003P_RADIO: {9000: ("Stop()",)},
        QF_003P_DUNCAN: {
            200: ("Alias_AssaultronKey.Clear()",),
            300: ("Alias_HandyKey.Clear()",),
        },
    }
    for script_name, stages in expected.items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        for stage, snippets in stages.items():
            body = _member_body(patch, _member_name(stage))
            for snippet in snippets:
                assert snippet in body

    lacey = _script_patch_source(QF_001P_LACEY)
    assert lacey is not None
    dispatch = _member_body(lacey, "dispatchwaywardstartevent")
    assert (
        "W05_MQ_001P_Wayward_QuestStartKeyword.SendStoryEventAndWait("
        "None, playerRef, playerRef)"
    ) in dispatch


def test_completed_early_quests_retry_successors_without_replaying_completion():
    wayward = _script_patch_source(QF_001P)
    radical = _script_patch_source(QF_002P)
    radical_controller = _script_patch_source("W05_002P_Radical_QuestScript")
    assert wayward is not None
    assert radical is not None
    assert radical_controller is not None

    for stage in (900, 905):
        body = _member_body(wayward, _member_name(stage))
        assert body.index("SetStage(9000)") < body.index("AttemptRadicalHandoff()")
        assert "SendStoryEventAndWait" not in body

    attempt = _member_body(wayward, "attemptradicalhandoff")
    target_state = "accepted = radicalQuest.IsRunning() || radicalQuest.IsCompleted()"
    wayward_send = (
        "W05_MQ_002P_Radical_QuestStartKeyword."
        "SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    assert attempt.count(target_state) == 2
    assert attempt.index(target_state) < attempt.index(wayward_send) < attempt.rindex(
        target_state
    )
    assert attempt.count(wayward_send) == 1
    assert f"accepted = {wayward_send}" not in attempt
    assert "If !accepted && IsStageDone(9000)" in attempt
    assert "StartTimer(5.0, 9000)" in attempt
    wayward_timer = _member_body(wayward, "ontimer")
    assert "aiTimerID == 9000 && IsStageDone(9000)" in wayward_timer
    assert wayward_timer.count("AttemptRadicalHandoff()") == 1
    assert wayward.count("CompleteQuest()") == 1

    radical_finish = _member_body(radical, _member_name(8950))
    assert "If !IsStageDone(9000)" in radical_finish
    assert "SetStage(9000)" in radical_finish
    assert "SendStoryEventAndWait" not in radical_finish

    try_start = _member_body(radical_controller, "trystartmusclequest")
    scene_guard = "If playerRef == None || playerRef.IsInScene()"
    muscle_send = "startKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    pre_send_state = (
        "If muscleQuest == None || muscleQuest.IsRunning() || "
        "muscleQuest.IsCompleted()"
    )
    post_send_state = "If muscleQuest.IsRunning() || muscleQuest.IsCompleted()"
    guard_start = try_start.index(scene_guard)
    guard_end = try_start.index("EndIf", guard_start)
    assert try_start.index(pre_send_state) < try_start.index(muscle_send)
    assert guard_start < guard_end < try_start.index(muscle_send)
    assert "StartTimer(1.0, 8950)" in try_start[guard_start:guard_end]
    assert try_start.count(muscle_send) == 1
    assert try_start.index(muscle_send) < try_start.index(post_send_state)
    assert f"= {muscle_send}" not in try_start
    assert f"&& {muscle_send}" not in try_start
    assert try_start.rstrip().endswith("StartTimer(1.0, 8950)\nEndFunction")
    radical_timer = _member_body(radical_controller, "ontimer")
    assert "aiTimerID == 8950" in radical_timer
    assert radical_timer.count("TryStartMuscleQuest()") == 1
    assert radical.count("CompleteQuest()") == 1


@pytest.mark.parametrize("script_name", ASSIGNED_SCRIPTS)
def test_assigned_production_merge_is_idempotent_and_native_compiles(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    skeleton = _production_skeleton(script_name)
    merged = _merge_script_method_patches(skeleton, patch)

    assert _merge_script_method_patches(merged, patch) == merged
    skeleton_members = Counter(_member_names(skeleton))
    patch_members = Counter(_member_names(patch))
    merged_members = Counter(_member_names(merged))
    for member_name in patch_members:
        assert merged_members[member_name] == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    for member_name in skeleton_members.keys() - patch_members.keys():
        assert merged_members[member_name] == skeleton_members[member_name]

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name", (QF_001P, QF_002P, QF_003P, QF_004P))
def test_each_completed_main_patch_has_no_plain_todo(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not [line for line in patch.splitlines() if line.strip() == "; TODO"]
