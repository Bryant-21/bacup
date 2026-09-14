from __future__ import annotations

import csv
from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
)


REPO_ROOT = Path(__file__).resolve().parents[5]
DOCS = REPO_ROOT / "bacup" / "docs" / "stub_restoration"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
QF = "Fragments:Quests:QF_W05_MQA_206P_0054EDB9"
CONTRACT = "contracts/w05-mqa-206p-jenny-alias-shell-closure.md"


def _member_names(source: str) -> set[str]:
    return {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    }


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if kind in {"function", "event"} and name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def test_jenny_alias_script_is_an_unpatched_nondefect_shell() -> None:
    with (DOCS / "status.csv").open(encoding="utf-8", newline="") as stream:
        row = next(
            row
            for row in csv.DictReader(stream)
            if row["relative_path"] == "W05_MQA_206P_JennyRefScript.psc"
        )

    assert row["terminal_state"] == "non-defect"
    assert row["evidence"] == CONTRACT
    assert _script_patch_source("W05_MQA_206P_JennyRefScript") is None

    generated = (
        SOURCE_ROOT / "W05_MQA_206P_JennyRefScript.psc"
    ).read_text(encoding="utf-8")
    assert generated.strip() == (
        "Scriptname W05_MQA_206P_JennyRefScript Extends ReferenceAlias"
    )
    assert _member_names(generated) == set()


def test_complete_qf_owns_jens_stage_60_to_70_lifecycle() -> None:
    patch = _script_patch_source(QF)
    assert patch is not None
    assert len(_member_names(patch)) == 74
    assert "TODO" not in patch

    stage_60 = _member_body(patch, "fragment_stage_0060_item_00")
    assert "playerRef.SetValue(W05_MQA_206P_JenGoneAV, 1.0)" in stage_60
    assert "jenRef.AddSpell(W05_MQS_205P_JenStealthSpell)" in stage_60
    assert "jenRef.EvaluatePackage()" in stage_60

    stage_70 = _member_body(patch, "fragment_stage_0070_item_00")
    assert "playerRef.GetItemCount(W05_MQA_206P_JensNote) == 0" in stage_70
    assert "playerRef.AddItem(W05_MQA_206P_JensNote, 1, True)" in stage_70
    assert "jenRef.RemoveSpell(W05_MQS_205P_JenStealthSpell)" in stage_70
    assert "jenRef.Disable()" in stage_70
