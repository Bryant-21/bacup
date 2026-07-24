from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
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
QF_002P = "Fragments:Quests:QF_W05_MQ_002P_Radical_0040F5BE"
QF_003P = "Fragments:Quests:QF_W05_MQ_003P_Muscle_0041A39D"
QF_004P = "Fragments:Quests:QF_W05_MQ_004P_Crane_0041C976"


def _member_name(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


def _function_body(stage: int, *lines: str) -> str:
    body = "\n".join(f"    {line}" if line else "" for line in lines)
    return (
        f"Function Fragment_Stage_{stage:04d}_Item_00()\n"
        f"{body}\n"
        "EndFunction"
    )


OBJECTIVE_STAGES: dict[str, tuple[int, ...]] = {
    QF_000P: (),
    QF_001P: (200, 300, 400, 600),
    QF_002P: (
        100,
        110,
        125,
        130,
        200,
        400,
        475,
        700,
        998,
        1000,
        1020,
        1030,
        1210,
        1310,
        1320,
        1600,
        2000,
    ),
    QF_003P: (
        100,
        150,
        200,
        300,
        400,
        500,
        600,
        700,
        1000,
        1020,
        1100,
        1150,
        1200,
        1205,
        1300,
        1390,
    ),
    QF_004P: (100, 110, 111, 300, 399, 500, 700, 750, 760, 800, 1000, 1100, 1200, 1230),
}

REPAIR_BODIES: dict[str, dict[int, str]] = {
    QF_000P: {
        2100: _function_body(
            2100,
            "Actor playerRef = Alias_Player.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_PlayerKnows_BeenToVault79, 1.0)",
            "EndIf",
            "If IsStageDone(2200) && !IsStageDone(2300)",
            "    SetStage(2300)",
            "EndIf",
        ),
        2200: _function_body(
            2200,
            "Actor playerRef = Alias_Player.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(pW05_MQ00_CodeAV, 1.0)",
            "EndIf",
            "If IsStageDone(2100) && !IsStageDone(2300)",
            "    SetStage(2300)",
            "EndIf",
        ),
        2300: _function_body(
            2300,
            "Actor playerRef = Alias_Player.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(pW05_MQ00_Completed, 1.0)",
            "EndIf",
            "If !IsStageDone(9000)",
            "    SetStage(9000)",
            "EndIf",
        ),
    },
    QF_001P: {
        550: _function_body(
            550,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_001P_Wayward_PlayerLearnedRadicalsLocation, 1.0)",
            "EndIf",
        ),
        598: _function_body(
            598,
            "If W05_MQ_001P_Wayward_0599_DuchessInterstatial",
            "    W05_MQ_001P_Wayward_0599_DuchessInterstatial.Start()",
            "EndIf",
        ),
        610: _function_body(
            610,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.AddItem(Brew_DuchessDram, 1, False)",
            "    playerRef.SetValue(W05_MQ_001P_Wayward_PlayerGotFreeDrink, 1.0)",
            "EndIf",
        ),
        620: _function_body(
            620,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_001P_Wayward_PlayerAskedAboutOverseer, 1.0)",
            "EndIf",
        ),
        660: _function_body(
            660,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_001P_Wayward_PlayerNegotiatedBetterPrice_002, 1.0)",
            "EndIf",
        ),
        680: _function_body(
            680,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_001P_Wayward_PlayerAgreedToHearOutDuchess, 1.0)",
            "EndIf",
        ),
        710: _function_body(
            710,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_001P_Wayward_PlayerNegotiatedBetterPrice_003, 1.0)",
            "EndIf",
        ),
        809: _function_body(
            809,
            "ObjectReference schematicMarker = Alias_SchematicEnableMarker.GetReference()",
            "If schematicMarker",
            "    schematicMarker.Disable()",
            "EndIf",
        ),
        820: _function_body(
            820,
            "ObjectReference gorgeMapMarker = Alias_GorgeMapMarker.GetReference()",
            "If gorgeMapMarker",
            "    gorgeMapMarker.AddToMap(False)",
            "EndIf",
        ),
        500: _function_body(
            500,
            "If W05_MQ_001P_Wayward_500_Scene",
            "    W05_MQ_001P_Wayward_500_Scene.Start()",
            "EndIf",
        ),
        599: _function_body(
            599,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_001P_Wayward_BatterDied, 1.0)",
            "EndIf",
            "If !IsStageDone(598)",
            "    SetStage(598)",
            "EndIf",
            "If !IsStageDone(600)",
            "    SetStage(600)",
            "EndIf",
        ),
        805: _function_body(
            805,
            "Actor duchessRef = Alias_Duchess.GetActorReference()",
            "If duchessRef",
            "    duchessRef.EvaluatePackage()",
            "EndIf",
        ),
        807: _function_body(
            807,
            "If !IsStageDone(809)",
            "    SetStage(809)",
            "EndIf",
            "Actor duchessRef = Alias_Duchess.GetActorReference()",
            "If duchessRef",
            "    duchessRef.EvaluatePackage()",
            "EndIf",
        ),
        900: _function_body(900, "If !IsStageDone(9000)", "    SetStage(9000)", "EndIf"),
        905: _function_body(905, "If !IsStageDone(9000)", "    SetStage(9000)", "EndIf"),
        1000: _function_body(
            1000,
            "If W05_MQ_003P_Muscle_QuestStartKeyword",
            "    W05_MQ_003P_Muscle_QuestStartKeyword.SendStoryEvent()",
            "EndIf",
        ),
    },
    QF_002P: {
        140: _function_body(140, "SetObjectiveCompleted(125)"),
        505: _function_body(
            505,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_002P_Radical_PlayerWasAJerkToFirstEnc, 1.0)",
            "EndIf",
        ),
        510: _function_body(
            510,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_002P_Radical_FirstEncDismissed, 1.0)",
            "EndIf",
        ),
        610: _function_body(
            610,
            "If W05_MQ_002P_Radical_600_GangerScene",
            "    W05_MQ_002P_Radical_600_GangerScene.Start()",
            "EndIf",
        ),
        709: _function_body(
            709,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsRadicalsLocation, 1.0)",
            "EndIf",
        ),
        720: _function_body(
            720,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_002P_Radical_SecondEncDismissedFast, 1.0)",
            "EndIf",
        ),
        736: _function_body(
            736,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsPassword, 1.0)",
            "EndIf",
        ),
        1220: _function_body(
            1220,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsPassword, 1.0)",
            "EndIf",
        ),
        1240: _function_body(
            1240,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsPassword, 1.0)",
            "EndIf",
        ),
        1315: _function_body(
            1315,
            "If !IsStageDone(1320)",
            "    SetStage(1320)",
            "EndIf",
        ),
        1500: _function_body(
            1500,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_002P_Radical_SplitTreasureWithRoper, 1.0)",
            "EndIf",
        ),
        1700: _function_body(
            1700,
            "If !IsStageDone(2000)",
            "    SetStage(2000)",
            "EndIf",
        ),
        2115: _function_body(
            2115,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_001P_Wayward_PlayerNegotiatedBetterPrice_003, 1.0)",
            "EndIf",
        ),
        450: _function_body(
            450,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_002P_Radical_PlayerConnectedRadioStation, 1.0)",
            "EndIf",
            "If !IsStageDone(709)",
            "    SetStage(709)",
            "EndIf",
            "If !IsStageDone(736)",
            "    SetStage(736)",
            "EndIf",
            "If !IsStageDone(1000)",
            "    SetStage(1000)",
            "EndIf",
        ),
        1550: _function_body(1550, "If !IsStageDone(2000)", "    SetStage(2000)", "EndIf"),
        8950: _function_body(
            8950,
            "If W05_MQ_003P_Muscle_QuestStartKeyword",
            "    W05_MQ_003P_Muscle_QuestStartKeyword.SendStoryEvent()",
            "EndIf",
            "If !IsStageDone(9000)",
            "    SetStage(9000)",
            "EndIf",
        ),
    },
    QF_003P: {
        499: _function_body(
            499,
            "If W05_MQ_003P_Muscle_0500_SolExitsScene",
            "    W05_MQ_003P_Muscle_0500_SolExitsScene.Start()",
            "EndIf",
        ),
        550: _function_body(
            550,
            "ObjectReference gauleyMarker = Alias_Sol_GauleyMine_EnableMarker.GetReference()",
            "ObjectReference waywardMarker = Alias_Sol_Wayward_EnableMarker.GetReference()",
            "If gauleyMarker",
            "    gauleyMarker.Disable()",
            "EndIf",
            "If waywardMarker",
            "    waywardMarker.Enable()",
            "EndIf",
        ),
        999: _function_body(
            999,
            "If !IsStageDone(1000)",
            "    SetStage(1000)",
            "EndIf",
        ),
        1005: _function_body(
            1005,
            "If W05_MQ_003P_Muscle_1005_ReturnScene",
            "    W05_MQ_003P_Muscle_1005_ReturnScene.Start()",
            "EndIf",
        ),
        1224: _function_body(
            1224,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.AddItem(W05_MQ_003P_Muscle_AssaultronRoomCard, 1, False)",
            "EndIf",
        ),
        1225: _function_body(
            1225,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.AddItem(W05_MQ_003P_Muscle_HandyRoomKey, 1, False)",
            "EndIf",
        ),
        1230: _function_body(
            1230,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_003P_Muscle_PlayerKilledSkinner, 1.0)",
            "EndIf",
        ),
        1310: _function_body(
            1310,
            "If W05_MQ_003P_Muscle_1310_ReturnScene",
            "    W05_MQ_003P_Muscle_1310_ReturnScene.Start()",
            "EndIf",
        ),
        1311: _function_body(
            1311,
            "If W05_MQ_003P_Muscle_1311_RadioScene",
            "    W05_MQ_003P_Muscle_1311_RadioScene.Start()",
            "EndIf",
        ),
        1312: _function_body(
            1312,
            "If W05_MQ_003P_Muscle_1311_RadioScene",
            "    W05_MQ_003P_Muscle_1311_RadioScene.Stop()",
            "EndIf",
        ),
        1320: _function_body(
            1320,
            "If W05_MQ_003P_Muscle_1310_ReturnScene",
            "    W05_MQ_003P_Muscle_1310_ReturnScene.Stop()",
            "EndIf",
            "If W05_MQ_003P_Muscle_1320_PlayerReturnsScene",
            "    W05_MQ_003P_Muscle_1320_PlayerReturnsScene.Start()",
            "EndIf",
        ),
        1500: _function_body(
            1500,
            "If !IsStageDone(9000)",
            "    SetStage(9000)",
            "EndIf",
            "If W05_MQ_004P_Crane_QuestStartKeyword",
            "    W05_MQ_004P_Crane_QuestStartKeyword.SendStoryEvent()",
            "EndIf",
        ),
        710: _function_body(710, "If !IsStageDone(800)", "    SetStage(800)", "EndIf"),
        900: _function_body(900, "If !IsStageDone(1000)", "    SetStage(1000)", "EndIf"),
    },
    QF_004P: {
        103: _function_body(
            103,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef && playerRef.GetValue(W05_MQ_004P_PlayerStartedQuestOnce) < 1.0 && W05_MQ_004P_Crane_0100a_StartScene",
            "    W05_MQ_004P_Crane_0100a_StartScene.Start()",
            "EndIf",
        ),
        105: _function_body(
            105,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.SetValue(W05_MQ_004P_PlayerStartedQuestOnce, 1.0)",
            "EndIf",
        ),
        310: _function_body(
            310,
            "Actor solRef = Alias_Sol.GetActorReference()",
            "If solRef",
            "    solRef.EvaluatePackage()",
            "EndIf",
        ),
        400: _function_body(
            400,
            "If W05_MQ_004P_Crane_0400_MomentOfSilenceScene",
            "    W05_MQ_004P_Crane_0400_MomentOfSilenceScene.Start()",
            "EndIf",
        ),
        600: _function_body(
            600,
            "If IsStageDone(650) && !IsStageDone(700)",
            "    SetStage(700)",
            "EndIf",
        ),
        650: _function_body(
            650,
            "If IsStageDone(600) && !IsStageDone(700)",
            "    SetStage(700)",
            "EndIf",
        ),
        1170: _function_body(
            1170,
            "If FlatwoodsMapMarker",
            "    FlatwoodsMapMarker.AddToMap(False)",
            "EndIf",
        ),
        1180: _function_body(
            1180,
            "If MorgantownAirportMapMarker",
            "    MorgantownAirportMapMarker.AddToMap(False)",
            "EndIf",
        ),
        1245: _function_body(
            1245,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.RemoveItem(Caps001, 100, False)",
            "EndIf",
        ),
        1265: _function_body(
            1265,
            "Actor playerRef = Alias_owningPlayer.GetActorReference()",
            "If playerRef",
            "    playerRef.AddItem(Headwear_Radicals, 1, False)",
            "    playerRef.SetValue(W05_MQ_004P_Crane_PlayerReceivedRadicalsGear, 1.0)",
            "EndIf",
            "If !IsStageDone(1300)",
            "    SetStage(1300)",
            "EndIf",
        ),
        1300: _function_body(
            1300,
            "If W05_MQ_004P_Crane_1300_DuchessAttractScene",
            "    W05_MQ_004P_Crane_1300_DuchessAttractScene.Start()",
            "EndIf",
        ),
        1150: _function_body(1150, "If !Alias_Roper.GetReference() && !IsStageDone(1300)", "    SetStage(1300)", "EndIf"),
        1235: _function_body(1235, "If !IsStageDone(1300)", "    SetStage(1300)", "EndIf"),
        1240: _function_body(1240, "If !IsStageDone(1300)", "    SetStage(1300)", "EndIf"),
        1250: _function_body(1250, "If !IsStageDone(1300)", "    SetStage(1300)", "EndIf"),
        1260: _function_body(
            1260,
            "If !IsStageDone(1261)",
            "    SetStage(1261)",
            "EndIf",
            "If IsStageDone(1261) && !IsStageDone(1300)",
            "    SetStage(1300)",
            "EndIf",
        ),
        8999: _function_body(
            8999,
            "If !IsStageDone(9000)",
            "    SetStage(9000)",
            "EndIf",
            "If W05_MQ_101P_QuestStartKeyword",
            "    W05_MQ_101P_QuestStartKeyword.SendStoryEvent()",
            "EndIf",
        ),
    },
}

NEGATIVE_STAGES: dict[str, tuple[int, ...]] = {
    QF_000P: (
        1, 100, 150, 200, 250, 300, 350, 400, 450, 500, 550, 999, 1100,
        1150, 1200, 1250, 1300, 1350, 1400, 1450, 1500, 1550, 1600, 1650,
        1999, 9000,
    ),
    QF_001P: (
        10, 103, 105, 301, 302, 310, 445, 450, 455, 460, 470, 491,
        510, 515, 516, 520, 522, 530, 705,
        9000,
    ),
    QF_002P: (
        1, 2, 5, 6, 10, 150, 160, 270, 460, 498, 501, 502, 504, 511,
        515, 525, 707, 710, 725, 735, 745, 746, 760, 764, 765, 766, 799,
        1100, 1101, 1140, 1290, 1350, 1575, 9000,
    ),
    QF_003P: (
        1, 2, 3, 4, 5, 6, 7, 8, 10, 103, 410, 415, 450, 476, 715, 725,
        800, 1015, 1025, 1050, 1226, 1229, 1232, 1240, 1251, 1270, 1275,
        1280, 1321, 1325, 9000, 10000,
    ),
    QF_004P: (
        1, 2, 3, 4, 5, 6, 10, 50, 102, 108, 109, 112, 125, 200, 210, 301,
        401, 495, 701, 702, 703, 704, 710, 765, 775, 820, 830, 1105,
        1220, 1221, 1242, 1243, 1244, 1261,
        9000,
    ),
}

EXPECTED_TOTALS = {
    QF_000P: 29,
    QF_001P: 40,
    QF_002P: 67,
    QF_003P: 62,
    QF_004P: 66,
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


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(script_name), patch)


@pytest.mark.parametrize("script_name", EXPECTED_TOTALS)
def test_patch_has_exact_ordered_positive_allowlist(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "property "))
        for line in patch.splitlines()
    )
    assert _iter_papyrus_states(patch.splitlines()) == []

    expected_stages = OBJECTIVE_STAGES[script_name] + tuple(
        REPAIR_BODIES[script_name]
    )
    expected_names = [_member_name(stage) for stage in expected_stages]
    assert _member_names(patch) == expected_names
    assert Counter(_member_names(patch)) == Counter(expected_names)


@pytest.mark.parametrize("script_name", EXPECTED_TOTALS)
def test_repair_members_have_exact_reviewed_bodies(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    for stage, expected_body in REPAIR_BODIES[script_name].items():
        assert _member_body(patch, _member_name(stage)) == expected_body


@pytest.mark.parametrize("script_name", EXPECTED_TOTALS)
def test_positive_and_exact_negative_surfaces_cover_every_member(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    positive_names = set(_member_names(patch))
    negative_names = {_member_name(stage) for stage in NEGATIVE_STAGES[script_name]}

    assert positive_names.isdisjoint(negative_names)
    assert len(positive_names) + len(negative_names) == EXPECTED_TOTALS[script_name]
    assert len(negative_names) == len(NEGATIVE_STAGES[script_name])


def test_reviewer_corrections_and_record_owned_completion_stay_exact():
    qf_003p = _script_patch_source(QF_003P)
    qf_004p = _script_patch_source(QF_004P)
    assert qf_003p is not None
    assert qf_004p is not None

    stage_999 = _member_body(qf_003p, _member_name(999))
    assert "SetStage(1000)" in stage_999
    assert "SetValue(" not in stage_999
    assert ".Say(" not in stage_999
    assert _member_name(701) not in _member_names(qf_004p)
    assert "W05_MQ_004P_Crane_0399_PlayerKilledCraneEarly" not in qf_004p

    for script_name in EXPECTED_TOTALS:
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert _member_name(9000) not in _member_names(patch)
        assert "CompleteQuest(" not in patch


def test_crane_intro_uses_only_bound_scene_and_owning_player_gate():
    patch = _script_patch_source(QF_004P)
    assert patch is not None
    stage_103 = _member_body(patch, _member_name(103))
    assert "Alias_owningPlayer.GetActorReference()" in stage_103
    assert "playerRef.GetValue(W05_MQ_004P_PlayerStartedQuestOnce) < 1.0" in stage_103
    assert stage_103.count("W05_MQ_004P_Crane_0100a_StartScene.Start()") == 1
    assert "W05_MQ_004P_Crane_0100_StartScene" not in stage_103


@pytest.mark.parametrize("script_name", EXPECTED_TOTALS)
def test_production_merge_is_exact_and_idempotent(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    skeleton = _production_skeleton(script_name)
    merged = _merge_script_method_patches(skeleton, patch)

    assert Counter(_member_names(merged)) == Counter(_member_names(patch))
    for member_name in _member_names(patch):
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    for stage in NEGATIVE_STAGES[script_name]:
        assert _member_name(stage) not in _member_names(merged)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", EXPECTED_TOTALS)
def test_full_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_exact_reviewed_accounting():
    assert sum(len(bodies) for bodies in REPAIR_BODIES.values()) == 66
    assert sum(len(stages) for stages in NEGATIVE_STAGES.values()) == 147
    assert {
        script_name: (
            len(OBJECTIVE_STAGES[script_name]),
            len(REPAIR_BODIES[script_name]),
            len(NEGATIVE_STAGES[script_name]),
        )
        for script_name in EXPECTED_TOTALS
    } == {
        QF_000P: (0, 3, 26),
        QF_001P: (4, 16, 20),
        QF_002P: (17, 16, 34),
        QF_003P: (16, 14, 32),
        QF_004P: (14, 17, 35),
    }
