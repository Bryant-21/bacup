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
QF = "Fragments:Quests:QF_W05_MQR_203P_0042F31B"
CONTRACT = "contracts/w05-mqr-203p-turret-alias-shell-closure.md"


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def test_property_free_alias_script_is_an_unpatched_nondefect_shell() -> None:
    with (DOCS / "status.csv").open(encoding="utf-8", newline="") as stream:
        row = next(
            row
            for row in csv.DictReader(stream)
            if row["relative_path"] == "W05_MQR_203P_TurretScript.psc"
        )
    assert row["terminal_state"] == "non-defect"
    assert row["evidence"] == CONTRACT
    assert _script_patch_source("W05_MQR_203P_TurretScript") is None

    generated = (SOURCE_ROOT / "W05_MQR_203P_TurretScript.psc").read_text(
        encoding="utf-8"
    )
    assert generated.strip() == (
        "Scriptname W05_MQR_203P_TurretScript Extends RefCollectionAlias"
    )


def test_qf_owns_round_activation_reset_and_shutdown_cleanup() -> None:
    patch = _script_patch_source(QF)
    assert patch is not None

    for stage in (1205, 1605):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert "Alias_NeutralTurrets.GetAt" in body
        assert "SetPlayerTeammate(True, False, False)" in body
        assert "Alias_AllyTurrets.AddRef(neutralTurret)" in body
        assert "allyTurret.Enable()" in body

    for stage in (1210, 1610, 10000):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert "Alias_AllyTurrets.GetAt" in body
        assert "SetPlayerTeammate(False, False, False)" in body

