from __future__ import annotations

import csv
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
)


REPO_ROOT = Path(__file__).resolve().parents[5]
DOCS = REPO_ROOT / "bacup" / "docs" / "stub_restoration"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
CONTRACT = "contracts/w05-mqs-201-203-controller-shell-closure.md"

CASES = {
    "W05_MQS_201P_QuestScript": {
        "path": "w05_mqs_201p_questscript.psc",
        "qf": "Fragments:Quests:QF_W05_MQSettlers_201P_Indus_003F28C3",
        "members": 59,
        "producer_stages": (427, 428, 429, 450, 730, 731, 951, 952, 953),
    },
    "W05_MQS_203P_QuestScript": {
        "path": "W05_MQS_203P_QuestScript.psc",
        "qf": "Fragments:Quests:QF_W05_MQS_203P_0040571C",
        "members": 48,
        "producer_stages": (700, 710, 720, 730, 1600),
    },
}


def _statuses() -> dict[str, dict[str, str]]:
    with (DOCS / "status.csv").open(encoding="utf-8", newline="") as stream:
        return {row["relative_path"].lower(): row for row in csv.DictReader(stream)}


def _member_names(source: str) -> set[str]:
    return {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    }


@pytest.mark.parametrize(("script_name", "case"), CASES.items())
def test_controller_is_an_unpatched_nondefect_shell(
    script_name: str, case: dict[str, object]
) -> None:
    row = _statuses()[str(case["path"]).lower()]
    assert row["terminal_state"] == "non-defect"
    assert row["evidence"] == CONTRACT
    assert _script_patch_source(script_name) is None

    generated = (SOURCE_ROOT / str(case["path"])).read_text(encoding="utf-8")
    assert _member_names(generated) == set()


@pytest.mark.parametrize(("script_name", "case"), CASES.items())
def test_complete_qf_patch_owns_every_local_stage_response(
    script_name: str, case: dict[str, object]
) -> None:
    del script_name
    qf_patch = _script_patch_source(str(case["qf"]))
    assert qf_patch is not None
    names = _member_names(qf_patch)
    assert len(names) == case["members"]
    assert "TODO" not in qf_patch
    for stage in case["producer_stages"]:
        assert f"fragment_stage_{stage:04d}_item_00" in names


def test_recovered_inventory_parent_supplies_the_runtime_producer() -> None:
    parent = _script_patch_source("DefaultAliasInventoryManagement")
    assert parent is not None
    names = _member_names(parent)
    assert {"onaliasinit", "onitemadded", "onitemremoved", "evaluateinventorystate"} <= names
    assert "TryToSetStage()" in parent
    assert "PrereqStage" in parent
    assert "TurnOffStage" in parent

