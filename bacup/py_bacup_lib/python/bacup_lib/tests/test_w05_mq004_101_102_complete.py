from __future__ import annotations

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
)


QUEST_STAGES = {
    "Fragments:Quests:QF_W05_MQ_004P_Crane_0041C976": (
        1, 2, 3, 4, 5, 6, 10, 50, 100, 102, 103, 105, 108, 109, 110, 111,
        112, 125, 200, 210, 300, 301, 310, 399, 400, 401, 495, 500, 600,
        650, 700, 701, 702, 703, 704, 710, 750, 760, 765, 775, 800, 820,
        830, 1000, 1100, 1105, 1150, 1170, 1180, 1200, 1220, 1221, 1230,
        1235, 1240, 1242, 1243, 1244, 1245, 1250, 1260, 1261, 1265, 1300,
        8999, 9000,
    ),
    "Fragments:Quests:QF_W05_MQ_101P_003FBBB2": (
        5, 10, 13, 15, 20, 30, 40, 50, 51, 52, 100, 110, 120, 150, 200,
        300, 400, 500, 550, 600, 700, 800, 805, 810, 820, 830, 900, 1000,
        1100, 1200, 1290, 1300, 1310, 1400, 1450, 1500, 1510, 1600, 1610,
        1700, 1800, 1810, 1820, 1900, 1910, 1920, 2000, 9000,
    ),
    "Fragments:Quests:QF_W05_MQ_102P_003FFACF": (
        10, 15, 20, 30, 200, 300, 400, 450, 530, 540, 550, 560, 565, 580,
        582, 584, 585, 586, 590, 595, 610, 615, 630, 640, 665, 680, 682,
        684, 685, 686, 690, 695, 700, 710, 720, 730, 740, 800, 850, 900,
        1000, 1200, 1300, 1400, 1500, 1600, 1700, 9000, 10000,
    ),
}


def _member_name(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _members(source: str) -> list[tuple[str, int, int]]:
    return [
        (name, start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind == "function"
    ]


def _body(source: str, stage: int) -> str:
    target = _member_name(stage)
    start, end = next(
        (start, end) for name, start, end in _members(source) if name == target
    )
    return "\n".join(source.splitlines()[start : end + 1])


@pytest.mark.parametrize(("script_name", "stages"), QUEST_STAGES.items())
def test_exact_live_bound_member_set_and_no_hollow_body(
    script_name: str, stages: tuple[int, ...]
):
    patch = _patch(script_name)
    members = _members(patch)
    names = [name for name, _start, _end in members]

    assert names == [_member_name(stage) for stage in stages]
    assert len(names) == len(set(names)) == len(stages)
    assert "; TODO" not in patch
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    for name, start, end in members:
        executable = [
            line.strip()
            for line in patch.splitlines()[start + 1 : end]
            if line.strip() and not line.lstrip().startswith(";")
        ]
        assert executable, f"hollow live-bound member: {script_name}.{name}"


def test_crane_route_reaches_cache_wrapup_and_next_main_quest():
    patch = _patch("Fragments:Quests:QF_W05_MQ_004P_Crane_0041C976")

    assert "SetStage(700)" in _body(patch, 600)
    assert "SetStage(700)" in _body(patch, 650)
    keypad = _body(patch, 765)
    assert "cacheDoor.Unlock()" in keypad
    assert "cacheDoor.SetOpen(True)" in keypad
    assert "SetStage(1000)" in keypad
    fight = _body(patch, 1230)
    assert "Alias_Radicals.GetCount()" in fight
    assert "radicalRef.StartCombat(playerRef)" in fight
    assert "SetStage(1261)" in _body(patch, 1260)
    completion = _body(patch, 9000)
    assert "W05_MQ_101P_QuestStartKeyword.SendStoryEvent" in completion
    assert "W05_WaywardSettlement_QuestStartKeyword.SendStoryEvent" in completion


def test_new_arrivals_route_repairs_reactor_flavor_assembly_and_both_deliveries():
    patch = _patch("Fragments:Quests:QF_W05_MQ_101P_003FBBB2")

    child_start = _body(patch, 100)
    assert "W05_MQ_101P_A_QuestStartKeyword.SendStoryEvent" in child_start
    assert "W05_MQ_101P_B_QuestStartKeyword.SendStoryEvent" in child_start
    for stage in (810, 820, 830):
        assert "SetStage(900)" in _body(patch, stage)
    assert "StartLocalEncounterWave(0)" in _body(patch, 1310)
    assert "SetStage(1450)" in _body(patch, 1000)
    assert "SetStage(1450)" in _body(patch, 1400)
    assert "W05_MQ_101P_011_CasesScene.Start()" in _body(patch, 1600)
    assert "SetStage(1610)" in _body(patch, 1600)
    assert "NukaColaVaccine_QuestObject.GetReference()" in _body(patch, 1610)
    assert "SetStage(2000)" in _body(patch, 1800)
    assert "SetStage(2000)" in _body(patch, 1900)
    assert "W05_MQ_102P_QuestStartKeyword.SendStoryEvent" in _body(patch, 9000)


def test_overseer_overseen_route_has_local_combat_reveal_and_faction_handoff():
    patch = _patch("Fragments:Quests:QF_W05_MQ_102P_003FFACF")

    assert "engineerRef.StartCombat(playerRef)" in _body(patch, 582)
    assert "staffRef.StartCombat(playerRef)" in _body(patch, 682)
    assert "SetStage(586)" in _body(patch, 584)
    assert "SetStage(686)" in _body(patch, 685)
    assert "W05_MQ_102P_009a_EstellaReveal.Start()" in _body(patch, 730)
    assert "SetStage(800)" in _body(patch, 740)
    handoff = _body(patch, 1500)
    assert "W05_MQ_102P_A_QuestStartKeyword.SendStoryEvent" in handoff
    assert "W05_MQ_102P_B_QuestStartKeyword.SendStoryEvent" in handoff
    assert "SetStage(9000)" in _body(patch, 1600)
    assert "SetStage(9000)" in _body(patch, 1700)
