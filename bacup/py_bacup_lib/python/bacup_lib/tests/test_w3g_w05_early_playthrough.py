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
        900: ("SetStage(9000)",),
        905: ("SetStage(9000)",),
        1000: ("W05_MQ_003P_Muscle_QuestStartKeyword.SendStoryEvent()",),
    },
    QF_002P: {
        450: (
            "SetValue(W05_MQ_002P_Radical_PlayerConnectedRadioStation, 1.0)",
            "SetStage(709)",
            "SetStage(736)",
            "SetStage(1000)",
        ),
        1550: ("SetStage(2000)",),
        8950: (
            "W05_MQ_003P_Muscle_QuestStartKeyword.SendStoryEvent()",
            "SetStage(9000)",
        ),
    },
    QF_003P: {
        100: ("W05_MQ_003P_Muscle_0100_StartScene.Start()", "SetStage(150)"),
        400: ("W05_MQ_003P_Muscle_0400_SolAttactScene.Start()",),
        710: ("SetStage(800)",),
        900: ("SetStage(1000)",),
        1500: (
            "SetStage(9000)",
            "W05_MQ_004P_Crane_QuestStartKeyword.SendStoryEvent()",
        ),
    },
    QF_004P: {
        1000: ("cacheDoor.Unlock()", "cacheDoor.SetOpen(True)"),
        1150: ("!Alias_Roper.GetReference()", "SetStage(1300)"),
        1200: ("W05_MQ_004P_Crane_1200_RoperScene.Start()",),
        1235: ("SetStage(1300)",),
        1240: ("SetStage(1300)",),
        1250: ("SetStage(1300)",),
        1260: ("SetStage(1261)", "IsStageDone(1261)", "SetStage(1300)"),
        1265: ("SetStage(1300)",),
        8999: ("SetStage(9000)", "W05_MQ_101P_QuestStartKeyword.SendStoryEvent()"),
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


def test_bypasses_land_only_on_source_named_receiving_stages():
    radical = _script_patch_source(QF_002P)
    muscle = _script_patch_source(QF_003P)
    crane = _script_patch_source(QF_004P)
    assert radical is not None
    assert muscle is not None
    assert crane is not None

    radical_450 = _member_body(radical, _member_name(450))
    assert "SetStage(709)" in radical_450
    assert "SetStage(736)" in radical_450
    assert "SetStage(1000)" in radical_450
    assert "SetStage(700)" not in radical_450
    assert "SetStage(800)" in _member_body(muscle, _member_name(710))
    assert "SetStage(900)" not in _member_body(muscle, _member_name(710))
    assert _member_name(820) not in _member_names(crane)
    assert "SetStage(1100)" not in _member_body(crane, _member_name(1000))

    bypass_bodies = "\n".join(
        (
            _member_body(radical, _member_name(450)),
            _member_body(muscle, _member_name(710)),
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
            10: ("Alias_owningPlayer.ForceRefIfEmpty(Game.GetPlayer())",),
            15: (
                "W05_MQ_001P_Wayward_QuestStartKeyword.SendStoryEventAndWait("
                "None, playerRef, playerRef)",
            ),
            100: ("W05_MQ_001P_Wayward.SetStage(200)",),
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


@pytest.mark.parametrize("script_name", ASSIGNED_SCRIPTS)
def test_assigned_production_merge_is_idempotent_and_native_compiles(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    skeleton = _production_skeleton(script_name)
    merged = _merge_script_method_patches(skeleton, patch)

    assert _merge_script_method_patches(merged, patch) == merged
    assert Counter(_member_names(merged)) == Counter(_member_names(patch))

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


@pytest.mark.parametrize("script_name", (QF_000P, QF_001P, QF_002P, QF_003P, QF_004P))
def test_each_still_partial_main_patch_keeps_one_plain_todo(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert [line for line in patch.splitlines() if line.strip() == "; TODO"] == [
        "; TODO"
    ]
