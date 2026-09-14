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

REPAIR_LINES = {
    QF_101P: {
        300: ("SetObjectiveCompleted(40)", "If IsStageDone(200)", "    SetStage(400)", "EndIf"),
        1000: ("If IsStageDone(1400)", "    SetStage(1450)", "EndIf"),
        1400: ("If IsStageDone(1000)", "    SetStage(1450)", "EndIf"),
        1600: ("SetStage(1610)",),
        1700: ("SetObjectiveCompleted(170)", "SetObjectiveDisplayed(180)", "SetObjectiveDisplayed(190)"),
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
        910: ("playerRef.RemoveItem(W05_MQ_101P_A_DavidHolotapeMeeting, 1, True)",),
        930: ("playerRef.AddItem(W05_MQ_101P_A_HookUp, 1, True)",),
        1110: ("SetStage(1200)",),
        1210: ("playerRef.RemoveItem(W05_MQ_101P_A_DavidTrophy, 1, True)",),
        1420: ("SetStage(1500)",),
        1450: ("SetStage(1500)",),
        1530: ("playerRef.SetValue(W05_PlayerKnows_AppalachiaHasATreasure, 1.0)",),
        8000: ("playerRef.SetValue(W05_MQ_101P_A_AldridgeWatchstationValue, 1.0)",),
        9000: ("W05_MQ_101P.SetStage(200)",),
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
        450: ("SetObjectiveDisplayed(250)",),
        560: (
            "ObjectReference playerRef = Alias_currentPlayer.GetReference()",
            "If playerRef && playerRef.GetItemCount(W05_MQ_102P_VTec_Holotape02) == 0",
            "    playerRef.AddItem(W05_MQ_102P_VTec_Holotape02, 1, False)",
            "EndIf",
        ),
        580: ("W05_MQ_102P_007a_ArrestBrass.Start()",),
        584: ("If IsStageDone(585)", "    SetStage(586)", "EndIf"),
        585: ("If IsStageDone(584)", "    SetStage(586)", "EndIf"),
        586: ("W05_MQ_102P_007c_DeathAftermath.Start()",),
        590: ("W05_MQ_102P_008a_BrassConfession.Start()",),
        610: (
            "ObjectReference playerRef = Alias_currentPlayer.GetReference()",
            "If playerRef && playerRef.GetItemCount(W05_MQ_102P_ReactorKey) == 0",
            "    playerRef.AddItem(W05_MQ_102P_ReactorKey, 1, False)",
            "EndIf",
        ),
        630: (
            "ObjectReference playerRef = Alias_currentPlayer.GetReference()",
            "If playerRef && playerRef.GetItemCount(W05_MQ_102P_LorisNote) == 0",
            "    playerRef.AddItem(W05_MQ_102P_LorisNote, 1, False)",
            "EndIf",
        ),
        680: ("W05_MQ_102P_007b_ArrestLoris.Start()",),
        684: ("If IsStageDone(685)", "    SetStage(686)", "EndIf"),
        685: ("If IsStageDone(684)", "    SetStage(686)", "EndIf"),
        686: ("W05_MQ_102P_007c_DeathAftermath.Start()",),
        690: ("W05_MQ_102P_008b_LorisConfession.Start()",),
        730: ("W05_MQ_102P_009a_EstellaReveal.Start()",),
        1000: (
            "If W05_MQ_102P_012a_Maintenance && !W05_MQ_102P_012a_Maintenance.IsPlaying()",
            "    W05_MQ_102P_012a_Maintenance.Start()",
            "EndIf",
        ),
        1200: (
            "If W05_MQ_102P_012_PresentationRoom && !W05_MQ_102P_012_PresentationRoom.IsPlaying()",
            "    W05_MQ_102P_012_PresentationRoom.Start()",
            "EndIf",
        ),
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

EXPECTED_STAGE_ORDER = {
    QF_101P: (5, 10, 13, 15, 20, 30, 40, 50, 51, 52, 100, 110, 120, 150, 200, 300, 400, 500, 550, 600, 700, 800, 805, 810, 820, 830, 900, 1000, 1100, 1200, 1290, 1300, 1310, 1400, 1450, 1500, 1510, 1600, 1610, 1700, 1800, 1810, 1820, 1900, 1910, 1920, 2000, 9000),
    QF_101P_A: (0, 1, 2, 3, 4, 5, 6, 10, 50, 100, 100, 200, 300, 310, 311, 320, 330, 331, 350, 375, 400, 500, 600, 650, 680, 700, 710, 730, 800, 810, 820, 830, 900, 910, 930, 950, 960, 970, 1000, 1050, 1100, 1110, 1200, 1210, 1300, 1400, 1415, 1420, 1430, 1440, 1450, 1500, 1530, 8000, 9000),
    QF_101P_B: (10, 100, 200, 230, 231, 232, 240, 300, 350, 400, 450, 500, 590, 600, 700, 9000),
    QF_102P: (10, 15, 20, 30, 200, 300, 400, 450, 530, 540, 550, 560, 565, 580, 582, 584, 585, 586, 590, 595, 610, 615, 630, 640, 665, 680, 682, 684, 685, 686, 690, 695, 700, 710, 720, 730, 740, 800, 850, 900, 1000, 1200, 1300, 1400, 1500, 1600, 1700, 9000, 10000),
}

def _fragment_member(stage: int, item: int = 0) -> str:
    return f"fragment_stage_{stage:04d}_item_{item:02d}"


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


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(script_name), patch)


@pytest.mark.parametrize("script_name", REPAIR_LINES)
def test_exact_live_fragment_surface(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    expected = _members_for_stages(EXPECTED_STAGE_ORDER[script_name])
    names = _member_names(patch)

    assert _iter_papyrus_states(patch.splitlines()) == []
    assert not any(line.strip().lower().startswith("scriptname ") for line in patch.splitlines())
    assert names == expected
    assert Counter(names) == Counter({name: 1 for name in expected})


@pytest.mark.parametrize("script_name", REPAIR_LINES)
def test_all_required_repair_effects_are_present_in_order(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    for stage, lines in REPAIR_LINES[script_name].items():
        body = _member_body(patch, _fragment_member(stage))
        position = 0
        for line in lines:
            next_position = body.find(line.strip(), position)
            assert next_position >= position, f"stage {stage} missing or reordered: {line}"
            position = next_position + len(line.strip())


def test_101p_stage_200_completes_branch_objective_then_converges():
    patch = _script_patch_source(QF_101P)
    assert patch is not None
    assert _member_body(patch, _fragment_member(200)) == """Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(30)
    If IsStageDone(300) && !IsStageDone(400)
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
    raider_cleanup = _member_body(
        _script_patch_source(QF_101P_A), _fragment_member(710)
    )
    assert "RemoveItem(W05_MQ_101P_A_AIProgramBroken" in raider_cleanup
    assert "RemoveItem(W05_MQ_101P_A_AIProgramFixed" in raider_cleanup
    shutdown = _member_body(_script_patch_source(QF_102P), _fragment_member(10000))
    assert shutdown.count(".IsPlaying()") == 3
    assert shutdown.count(".Stop()") == 3
    for forbidden in ("CompleteQuest(", "6313B7", "6313B8", "6313BA", "6313BB"):
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
