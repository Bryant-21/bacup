from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
)


REPO_ROOT = Path(__file__).resolve().parents[5]
TODO_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "TODO.md"
LEGACY_TEST = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "python"
    / "bacup_lib"
    / "tests"
    / "test_w3d_w05_mqs_script_patches.py"
)

QF_CASES = {
    "Fragments:Quests:QF_W05_MQSettlers_201P_Indus_003F28C3": {
        "record": "3F28C3",
        "patch": "QF_W05_MQSettlers_201P_Indus_003F28C3.psc",
        "implemented": (1, 10, 100, 125, 150, 175, 200, 225, 250, 300, 310, 311, 325, 400, 425, 426, 427, 428, 429, 430, 431, 432, 449, 450, 490, 500, 510, 525, 550, 600, 700, 625, 725, 730, 731, 750, 800, 810, 825, 850, 851, 875, 900, 901, 902, 925, 950, 951, 952, 953, 975, 1000, 1010, 1025, 1200, 1225, 9000, 9999, 10000),
    },
    "Fragments:Quests:QF_W05_MQS_202P_Acrobat_003F28C7": {
        "record": "3F28C7",
        "patch": "QF_W05_MQS_202P_Acrobat_003F28C7.psc",
        "implemented": (10, 50, 51, 100, 75, 150, 200, 201, 202, 210, 211, 225, 250, 251, 275, 300, 500, 600, 700, 720, 721, 725, 726, 727, 730, 746, 747, 748, 749, 750, 751, 752, 800, 851, 899, 900, 999, 9000, 9999, 10000),
    },
    "Fragments:Quests:QF_W05_MQS_203P_0040571C": {
        "record": "40571C",
        "patch": "QF_W05_MQS_203P_0040571C.psc",
        "implemented": (10, 100, 200, 300, 301, 310, 400, 500, 600, 700, 710, 720, 730, 800, 801, 900, 905, 910, 920, 921, 930, 931, 940, 941, 950, 1000, 1001, 1002, 1003, 1004, 1005, 1100, 1200, 1300, 1400, 1500, 1501, 1510, 1520, 1530, 1600, 1700, 1800, 2000, 2100, 9000, 9999, 10000),
    },
    "Fragments:Quests:QF_W05_MQS_Choice_00592500": {
        "record": "592500",
        "patch": "qf_w05_mqs_choice_00592500.psc",
        "implemented": (100, 9000, 9999),
    },
    "Fragments:Quests:QF_W05_MQS_204P_0040C458": {
        "record": "40C458",
        "patch": "QF_W05_MQS_204P_0040C458.psc",
        "implemented": (10, 15, 90, 100, 200, 210, 250, 275, 300, 360, 370, 375, 380, 390, 400, 500, 525, 550, 600, 610, 615, 620, 625, 630, 635, 640, 645, 650, 655, 700, 800, 900, 1000, 1100, 9000, 10000),
    },
    "Fragments:Quests:QF_W05_MQS_205P_0041CB6D": {
        "record": "41CB6D",
        "patch": "QF_W05_MQS_205P_0041CB6D.psc",
        "implemented": (20, 10, 30, 40, 50, 100, 200, 250, 300, 350, 400, 450, 500, 700, 800, 900, 950, 1000, 1050, 1100, 1300, 1400, 1500, 1600, 1800, 1900, 1950, 2000, 2100, 2200, 2300, 9000, 10000),
    },
}

CONTROLLERS = {
    "W05_MQS_201P_QuestScript": "3F28C3",
    "W05_MQS_203P_QuestScript": "40571C",
    "W05_MQS_204P_QuestScript": "40C458",
}


def _entries(prefix: str) -> list[dict[str, str]]:
    entries: list[dict[str, str]] = []
    for line in TODO_PATH.read_text(encoding="utf-8").splitlines():
        if not line.startswith(prefix):
            continue
        entries.append(dict(field.split("=", 1) for field in line.split("|")[1:]))
    return entries


def _stages(value: str) -> tuple[int, ...]:
    return tuple(int(member.split(":", 1)[0]) for member in value.split(","))


def _member_stages(source: str) -> tuple[int, ...]:
    stages: list[int] = []
    for kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines()):
        if kind not in {"function", "event"}:
            continue
        prefix = "fragment_stage_"
        assert name.startswith(prefix) and name.endswith("_item_00"), name
        stages.append(int(name[len(prefix) : -len("_item_00")]))
    return tuple(stages)


@pytest.mark.parametrize(("script_name", "case"), QF_CASES.items())
def test_each_mqs_qf_has_its_exact_complete_live_surface(script_name: str, case: dict):
    patch = _script_patch_source(script_name)
    assert patch is not None

    assert _member_stages(patch) == case["implemented"]
    assert [line for line in patch.splitlines() if "TODO" in line] == []
    assert "TODO(" not in patch


def test_mqs_qf_surface_accounting_is_exact():
    implemented = sum(len(case["implemented"]) for case in QF_CASES.values())
    assert implemented == 219


def test_genuinely_memberless_controllers_remain_unpatched():
    assert all(_script_patch_source(script_name) is None for script_name in CONTROLLERS)

    legacy_source = LEGACY_TEST.read_text(encoding="utf-8")
    assert "TODO(" not in legacy_source
    assert "# scripts=" not in legacy_source
    assert "# blocker=" not in legacy_source
