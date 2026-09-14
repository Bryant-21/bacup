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
QF = "Fragments:Quests:QF_W05_MQ_001P_Wayward_00405E14"
CONTRACT = "contracts/wayward-player-kill-alias-shell-closure.md"


def _members(source: str) -> set[tuple[str, str]]:
    return {
        (kind, name)
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


def test_player_kill_alias_is_an_unpatched_nondefect_shell() -> None:
    with (DOCS / "status.csv").open(encoding="utf-8", newline="") as stream:
        row = next(
            row
            for row in csv.DictReader(stream)
            if row["relative_path"] == "W05_001P_PlayerKillAliasScript.psc"
        )

    assert row["terminal_state"] == "non-defect"
    assert row["evidence"] == CONTRACT
    assert _script_patch_source("W05_001P_PlayerKillAliasScript") is None

    generated = (
        SOURCE_ROOT / "W05_001P_PlayerKillAliasScript.psc"
    ).read_text(encoding="utf-8")
    assert generated.strip() == (
        "Scriptname W05_001P_PlayerKillAliasScript Extends ReferenceAlias"
    )
    assert _members(generated) == set()


def test_batter_alias_and_complete_qf_own_the_kill_route() -> None:
    batter = _script_patch_source("W05_001P_BatterAliasScript")
    assert batter is not None
    assert ("event", "ondeath") in _members(batter)
    death = _member_body(batter, "ondeath")
    assert "W05_MQ_001P_Wayward_PlayerKilledBatter" in death
    assert "W05_MQ_001P_Wayward_MortKilledBatter" in death
    assert death.index("owningQuest.SetStage(470)") < death.index(
        "owningQuest.SetStage(599)"
    )

    qf = _script_patch_source(QF)
    assert qf is not None
    fragment_members = {
        name
        for kind, name in _members(qf)
        if kind == "function" and name.startswith("fragment_stage_")
    }
    assert len(fragment_members) == 40
    stage_599 = _member_body(qf, "fragment_stage_0599_item_00")
    assert "SetStage(598)" in stage_599
    assert "SetStage(600)" in stage_599
