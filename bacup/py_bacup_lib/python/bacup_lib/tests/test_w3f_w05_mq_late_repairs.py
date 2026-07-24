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
GENERATED_SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

QF_101P = "Fragments:Quests:QF_W05_MQ_101P_003FBBB2"
QF_101P_A = "Fragments:Quests:QF_W05_MQ_101P_A_003FBC0D"
QF_101P_B = "Fragments:Quests:QF_W05_MQ_101P_B_003FBC10"
QF_102P = "Fragments:Quests:QF_W05_MQ_102P_003FFACF"

OBJECTIVE_STAGES = {
    QF_101P: (10, 13, 15, 20, 30, 40, 50, 100, 110, 120, 150, 200),
    QF_101P_A: (
        50,
        100,
        100,
        200,
        300,
        350,
        400,
        500,
        600,
        650,
        700,
        800,
        900,
        950,
        970,
        1000,
        1100,
        1200,
        1300,
        1400,
        1500,
    ),
    QF_101P_B: (10,),
    QF_102P: (10, 20, 200, 300, 400, 530, 540, 550),
}

REPAIR_LINES = {
    QF_101P: {
        300: ("SetStage(351)", "If IsStageDone(200)", "    SetStage(400)", "EndIf"),
        1000: ("If IsStageDone(1400)", "    SetStage(1450)", "Else", "    SetStage(1410)", "EndIf"),
        1400: ("If IsStageDone(1000)", "    SetStage(1450)", "Else", "    SetStage(1420)", "EndIf"),
        1600: ("SetStage(1610)",),
        1700: ("SetStage(1750)",),
        1800: ("If IsStageDone(1900)", "    SetStage(2000)", "EndIf"),
        1900: ("If IsStageDone(1800)", "    SetStage(2000)", "EndIf"),
        9000: (
            "ObjectReference playerRef = Alias_currentPlayer.GetReference()",
            "W05_MQ_102P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)",
        ),
        5: ("If !IsStageDone(10) && !IsStageDone(20)", "    SetStage(10)", "EndIf"),
        600: (
            "If W05_MQ_101P_003_ColaPlantEntranceScene && !W05_MQ_101P_003_ColaPlantEntranceScene.IsPlaying()",
            "    W05_MQ_101P_003_ColaPlantEntranceScene.Start()",
            "EndIf",
        ),
        1300: (
            "If W05_MQ_101P_005b_OverseerHelps && !W05_MQ_101P_005b_OverseerHelps.IsPlaying()",
            "    W05_MQ_101P_005b_OverseerHelps.Start()",
            "EndIf",
        ),
        1310: ("If !IsStageDone(1400)", "    SetStage(1400)", "EndIf"),
    },
    QF_101P_A: {
        910: ("Alias_currentPlayer.GetReference().RemoveItem(W05_MQ_101P_A_DavidHolotapeMeeting, 1, True)",),
        930: ("Alias_currentPlayer.GetReference().AddItem(W05_MQ_101P_A_HookUp, 1, True)",),
        1110: (
            "Actor megRef = Alias_Meg.GetActorReference()",
            "If megRef",
            "    megRef.Enable()",
            "    megRef.EvaluatePackage()",
            "    SetStage(1200)",
            "EndIf",
        ),
        1210: ("Alias_currentPlayer.GetReference().RemoveItem(W05_MQ_101P_A_DavidTrophy, 1, True)",),
        1420: ("SetStage(1500)",),
        1450: ("SetStage(1500)",),
        1530: ("Alias_currentPlayer.GetReference().SetValue(W05_PlayerKnows_AppalachiaHasATreasure, 1.0)",),
        8000: ("Alias_currentPlayer.GetReference().SetValue(W05_MQ_101P_A_AldridgeWatchstationValue, 1.0)",),
        9000: ("W05_MQ_101P.SetStage(200)",),
        10: (
            "If MTNS01_Intro && MTNS01_Intro.IsStageDone(600)",
            "    SetStage(100)",
            "Else",
            "    SetStage(50)",
            "    ObjectReference playerRef = Alias_currentPlayer.GetReference()",
            "    MTNS01_Intro_Quest_Keyword.SendStoryEvent(None, playerRef, playerRef)",
            "EndIf",
        ),
        1050: ("If !IsStageDone(1110)", "    SetStage(1110)", "EndIf"),
    },
    QF_101P_B: {
        230: ("SetStage(300)",),
        231: ("SetStage(300)",),
        400: ("SetStage(700)",),
        450: ("SetStage(700)",),
        500: ("SetStage(700)",),
        600: ("SetStage(700)",),
        9000: ("W05_MQ_101P.SetStage(300)",),
    },
    QF_102P: {
        580: ("W05_MQ_102P_007a_ArrestBrass.Start()",),
        584: ("If IsStageDone(585)", "    SetStage(586)", "EndIf"),
        585: ("If IsStageDone(584)", "    SetStage(586)", "EndIf"),
        586: ("W05_MQ_102P_007c_DeathAftermath.Start()",),
        590: ("W05_MQ_102P_008a_BrassConfession.Start()",),
        680: ("W05_MQ_102P_007b_ArrestLoris.Start()",),
        684: ("If IsStageDone(685)", "    SetStage(686)", "EndIf"),
        685: ("If IsStageDone(684)", "    SetStage(686)", "EndIf"),
        686: ("W05_MQ_102P_007c_DeathAftermath.Start()",),
        690: ("W05_MQ_102P_008b_LorisConfession.Start()",),
        730: ("W05_MQ_102P_009a_EstellaReveal.Start()",),
        1300: ("W05_MQ_102P_013_Vault79PresentationScene.Start()",),
        15: (
            "ObjectReference actorEnableMarker = Alias_VaultTecUActorEnableMarker.GetReference()",
            "If actorEnableMarker",
            "    actorEnableMarker.Enable()",
            "EndIf",
            "If W05_MQ_102p_NPCEnableMarker",
            "    W05_MQ_102p_NPCEnableMarker.Enable()",
            "EndIf",
        ),
        30: (
            "If W05_MQ_102P_EnteredScene && !W05_MQ_102P_EnteredScene.IsPlaying()",
            "    W05_MQ_102P_EnteredScene.Start()",
            "EndIf",
        ),
        1400: ("If !IsStageDone(1500)", "    SetStage(1500)", "EndIf"),
        1500: (
            "ObjectReference playerRef = Alias_currentPlayer.GetReference()",
            "W05_MQ_102P_A_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)",
            "W05_MQ_102P_B_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)",
        ),
        1600: (
            "If IsStageDone(1700) && !IsStageDone(9000)",
            "    SetStage(9000)",
            "EndIf",
        ),
        1700: (
            "If IsStageDone(1600) && !IsStageDone(9000)",
            "    SetStage(9000)",
            "EndIf",
        ),
    },
}

NEGATIVE_STAGES = {
    QF_101P: (51, 52, 400, 500, 550, 700, 800, 805, 810, 820, 830, 900, 1100, 1200, 1290, 1450, 1500, 1510, 1610, 1810, 1820, 1910, 1920, 2000),
    QF_101P_A: (0, 1, 2, 3, 4, 5, 6, 310, 311, 320, 330, 331, 375, 680, 710, 730, 810, 820, 830, 960, 1415, 1430, 1440),
    QF_101P_B: (100, 200, 232, 240, 300, 350, 590, 700),
    QF_102P: (450, 560, 565, 582, 595, 610, 615, 630, 640, 665, 682, 695, 700, 710, 720, 740, 800, 850, 900, 1000, 1200, 9000, 10000),
}

EXPECTED_STAGE_ORDER = {
    QF_101P: (5, 10, 13, 15, 20, 30, 40, 50, 100, 110, 120, 150, 200, 300, 600, 1000, 1400, 1300, 1310, 1600, 1700, 1800, 1900, 9000),
    QF_101P_A: (10, 50, 100, 100, 200, 300, 350, 400, 500, 600, 650, 700, 800, 900, 950, 970, 1000, 1050, 1100, 1200, 1300, 1400, 1500, 910, 930, 1110, 1210, 1420, 1450, 1530, 8000, 9000),
    QF_101P_B: (10, 230, 231, 400, 450, 500, 600, 9000),
    QF_102P: (10, 15, 20, 30, 200, 300, 400, 530, 540, 550, 580, 584, 585, 586, 590, 680, 684, 685, 686, 690, 730, 1300, 1400, 1500, 1600, 1700),
}

LIVE_MEMBER_COUNTS = {QF_101P: 48, QF_101P_A: 55, QF_101P_B: 16, QF_102P: 49}


def _fragment_member(stage: int, item: int = 0) -> str:
    return f"fragment_stage_{stage:04d}_item_{item:02d}"


def _objective_members(script_name: str) -> list[str]:
    members: list[str] = []
    seen: dict[int, int] = {}
    for stage in OBJECTIVE_STAGES[script_name]:
        item = seen.get(stage, 0)
        members.append(_fragment_member(stage, item))
        seen[stage] = item + 1
    return members


def _members_for_stages(stages: tuple[int, ...]) -> list[str]:
    seen: dict[int, int] = {}
    members: list[str] = []
    for stage in stages:
        item = seen.get(stage, 0)
        members.append(_fragment_member(stage, item))
        seen[stage] = item + 1
    return members


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines())
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(source.splitlines())
        if kind in {"function", "event"} and name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _expected_body(stage: int, lines: tuple[str, ...]) -> str:
    body = "\n".join(f"    {line}" for line in lines)
    return f"Function Fragment_Stage_{stage:04d}_Item_00()\n{body}\nEndFunction"


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(script_name), patch)


@pytest.mark.parametrize("script_name", REPAIR_LINES)
def test_exact_positive_allowlist_and_negative_absence(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    expected_allowlist = _objective_members(script_name) + [
        _fragment_member(stage) for stage in REPAIR_LINES[script_name]
    ]
    expected = _members_for_stages(EXPECTED_STAGE_ORDER[script_name])
    names = _member_names(patch)

    assert set(expected) == set(expected_allowlist)
    assert _iter_papyrus_states(patch.splitlines()) == []
    assert not any(line.strip().lower().startswith("scriptname ") for line in patch.splitlines())
    assert names == expected
    assert Counter(names) == Counter({name: 1 for name in expected})
    assert set(names).isdisjoint({_fragment_member(stage) for stage in NEGATIVE_STAGES[script_name]})
    assert len(expected) + len(NEGATIVE_STAGES[script_name]) == LIVE_MEMBER_COUNTS[script_name]


@pytest.mark.parametrize("script_name", REPAIR_LINES)
def test_all_repair_bodies_are_exact(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    for stage, lines in REPAIR_LINES[script_name].items():
        assert _member_body(patch, _fragment_member(stage)) == _expected_body(stage, lines)


def test_101p_stage_200_preserves_objective_then_adds_symmetric_convergence():
    patch = _script_patch_source(QF_101P)
    assert patch is not None
    assert _member_body(patch, _fragment_member(200)) == """Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
    SetStage(350)
    If IsStageDone(300)
        SetStage(400)
    EndIf
EndFunction"""


def test_completion_fragments_do_not_fake_record_rewards_or_completion_flags():
    patches = "\n".join(
        line
        for script_name in REPAIR_LINES
        for line in (_script_patch_source(script_name) or "").splitlines()
        if not line.lstrip().startswith(";")
    )
    assert "Fragment_Stage_0710_Item_00" not in _script_patch_source(QF_101P_A)
    for forbidden in ("CompleteQuest(", "Stop()", "6313B7", "6313B8", "6313BA", "6313BB"):
        assert forbidden not in patches


@pytest.mark.parametrize("script_name", REPAIR_LINES)
def test_production_merge_is_exact_and_idempotent(script_name: str):
    skeleton = _production_skeleton(script_name)
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merge_script_method_patches(skeleton, patch)

    for member_name in _member_names(patch):
        assert _member_names(merged).count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    for line in skeleton.splitlines():
        if " property " in f" {line.lower()} ":
            assert line in merged
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", REPAIR_LINES)
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
