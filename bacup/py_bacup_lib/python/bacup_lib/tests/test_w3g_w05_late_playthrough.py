from __future__ import annotations

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
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)

QF_101P = "Fragments:Quests:QF_W05_MQ_101P_003FBBB2"
QF_101P_A = "Fragments:Quests:QF_W05_MQ_101P_A_003FBC0D"
QF_101P_B = "Fragments:Quests:QF_W05_MQ_101P_B_003FBC10"
QF_101P_RADIO = "Fragments:Quests:QF_W05_MQ_101P_Radio_003FBBB3"
QF_102P = "Fragments:Quests:QF_W05_MQ_102P_003FFACF"
QF_102P_A = "Fragments:Quests:QF_W05_MQ_102P_A_003FFC02"
QF_102P_B = "Fragments:Quests:QF_W05_MQ_102P_B_003FFC00"
QF_MQA = "Fragments:Quests:QF_W05_MQA_206P_0054EDB9"

PLAYTHROUGH_SCRIPTS = (
    QF_101P,
    QF_101P_A,
    QF_101P_B,
    QF_101P_RADIO,
    QF_102P,
    QF_102P_A,
    QF_102P_B,
    QF_MQA,
)

PROGRESSION_STAGES = {
    QF_101P: (5, 10, 30, 100, 600, 1300, 1310, 9000),
    QF_101P_A: (10, 1000, 1050, 1110, 9000),
    QF_101P_B: (9000,),
    QF_101P_RADIO: (20,),
    QF_102P: (10, 15, 30, 1300, 1400, 1500, 1600, 1700),
    QF_102P_A: (300, 9500),
    QF_102P_B: (9000,),
    QF_MQA: (30, 33, 50, 150, 200, 585, 590, 700, 800, 5000, 5050),
}


def _member_body(source: str, stage: int) -> str:
    name = f"fragment_stage_{stage:04d}_item_00"
    start, end = next(
        (start, end)
        for kind, member_name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind == "function" and member_name == name
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    return _merge_script_method_patches(
        _production_skeleton(script_name), _patch(script_name)
    )


def test_101p_start_children_and_102p_handoff_are_connected():
    mq = _patch(QF_101P)
    radio = _patch(QF_101P_RADIO)

    assert "SetStage(10)" in _member_body(mq, 5)
    assert "W05_MQ_101P_Radio_QuestStartKeyword.SendStoryEvent" in _member_body(
        mq, 10
    )
    assert "RS03_Inoculation_Keyword.SendStoryEvent" in _member_body(mq, 30)
    stage_100 = _member_body(mq, 100)
    assert "W05_MQ_101P_A_QuestStartKeyword.SendStoryEvent" in stage_100
    assert "W05_MQ_101P_B_QuestStartKeyword.SendStoryEvent" in stage_100
    assert "W05_MQ_101P.SetStage(200)" in _member_body(
        _patch(QF_101P_A), 9000
    )
    assert "W05_MQ_101P.SetStage(300)" in _member_body(
        _patch(QF_101P_B), 9000
    )
    assert "W05_MQ_101P.SetStage(20)" in _member_body(radio, 20)
    assert "W05_MQ_102P_QuestStartKeyword.SendStoryEvent" in _member_body(
        mq, 9000
    )


def test_instance_and_ews_gates_have_narrow_single_player_receivers():
    raider_intro = _patch(QF_101P_A)
    mq = _patch(QF_101P)
    vtu = _patch(QF_102P)

    assert "SetStage(1050)" in _member_body(raider_intro, 1000)
    assert "SetStage(1110)" in _member_body(raider_intro, 1050)
    assert "SetStage(1400)" in _member_body(mq, 1310)
    assert "SetStage(15)" in _member_body(vtu, 10)
    stage_15 = _member_body(vtu, 15)
    assert "Alias_VaultTecUActorEnableMarker.GetReference()" in stage_15
    assert "W05_MQ_102p_NPCEnableMarker.Enable()" in stage_15


def test_raider_intro_makes_meg_available_before_talk_objective():
    stage_1110 = _member_body(_patch(QF_101P_A), 1110)
    ordered_operations = (
        "Actor megRef = Alias_Meg.GetActorReference()",
        "If megRef",
        "megRef.Enable()",
        "megRef.EvaluatePackage()",
        "SetStage(1200)",
        "EndIf",
    )

    positions = [stage_1110.index(operation) for operation in ordered_operations]
    assert positions == sorted(positions)


def test_102p_children_start_next_chains_and_complete_parent():
    parent = _patch(QF_102P)
    stage_1500 = _member_body(parent, 1500)
    assert "W05_MQ_102P_A_QuestStartKeyword.SendStoryEvent" in stage_1500
    assert "W05_MQ_102P_B_QuestStartKeyword.SendStoryEvent" in stage_1500
    assert "SetStage(9000)" in _member_body(parent, 1600)
    assert "SetStage(9000)" in _member_body(parent, 1700)

    raiders = _member_body(_patch(QF_102P_A), 9500)
    settlers = _member_body(_patch(QF_102P_B), 9000)
    assert "W05_MQR_201P_QuestStart_Keyword.SendStoryEvent" in raiders
    assert "W05_MQ_102P.SetStage(1600)" in raiders
    assert "W05_MQS_201P_QuestStartKeyword.SendStoryEvent" in settlers
    assert "W05_MQ_102P.SetStage(1700)" in settlers


def test_mqa_scene_route_reaches_native_completion_without_rewards():
    mqa = _patch(QF_MQA)
    expected_scene_members = {
        30: "W05_MQA_206P_SeeChase.Start()",
        33: "W05_MQA_206P_Ghoul.Start()",
        50: "W05_MQA_206P_GoldRoom.Start()",
        150: "W05_MQA_206P_Greet.Start()",
        585: "W05_MQA_206P_LiveOrDie.Start()",
        700: "W05_MQA_206P_OperationsScene.Start()",
        5000: "W05_MQA_206P_Raiders_Leaving.Start()",
        5050: "W05_MQA_206P_Johnny_001_Gold.Start()",
    }
    for stage, scene_start in expected_scene_members.items():
        assert scene_start in _member_body(mqa, stage)

    stage_200 = _member_body(mqa, 200)
    assert "louDoor.SetActivatorOpen(True)" in stage_200
    assert "SetStage(250)" in stage_200
    assert "SetStage(600)" in _member_body(mqa, 590)
    assert "SetStage(9000)" in _member_body(mqa, 800)


def test_progression_members_do_not_recreate_online_or_reward_behavior():
    bodies = "\n".join(
        _member_body(_patch(script_name), stage)
        for script_name, stages in PROGRESSION_STAGES.items()
        for stage in stages
    )
    for forbidden in (
        "defaultquestencounterwavescript",
        "EncounterWaves",
        "CompleteQuest(",
        ".AddItem(",
        ".RemoveItem(",
        ".ModValue(",
        "Reputation_AV_",
        "GoldBullion",
    ):
        assert forbidden not in bodies


@pytest.mark.parametrize("script_name", PLAYTHROUGH_SCRIPTS)
def test_merged_progression_bodies_are_exact_and_unique(script_name: str):
    patch = _patch(script_name)
    merged = _merged_production_source(script_name)
    for stage in PROGRESSION_STAGES[script_name]:
        assert _member_body(merged, stage) == _member_body(patch, stage)
        member_name = f"fragment_stage_{stage:04d}_item_00"
        member_count = sum(
            kind == "function" and name == member_name
            for kind, name, _start, _end in _iter_top_level_papyrus_members(
                merged.splitlines()
            )
        )
        assert member_count == 1


def test_each_still_deferred_patch_has_one_plain_todo_marker():
    for script_name in PLAYTHROUGH_SCRIPTS:
        expected = 0 if script_name in {QF_101P_RADIO, QF_102P_A, QF_102P_B} else 1
        assert _patch(script_name).splitlines().count("; TODO") == expected


@pytest.mark.parametrize("script_name", PLAYTHROUGH_SCRIPTS)
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
