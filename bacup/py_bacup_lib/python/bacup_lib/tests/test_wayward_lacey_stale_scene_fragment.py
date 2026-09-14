from __future__ import annotations

import csv
from pathlib import Path

from bacup_lib.workflows.unified import _script_patch_source


REPO_ROOT = Path(__file__).resolve().parents[5]
STATUS = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
SCRIPT_PATH = "fragments/Quests/QF_W05_MQ_001P_Wayward_Lacey_00405E15.psc"
SCRIPT_NAME = "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_00405E15"
CONTRACT = "contracts/wayward-lacey-stale-scene-fragment.md"


def test_stale_scene_dependency_is_closed_on_the_existing_patch() -> None:
    with STATUS.open(encoding="utf-8", newline="") as stream:
        rows = {row["relative_path"]: row for row in csv.DictReader(stream)}

    assert rows[SCRIPT_PATH]["terminal_state"] == "patched"
    assert rows[SCRIPT_PATH]["evidence"] == CONTRACT
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "Function Fragment_Stage_0015_Item_00()" in patch
    assert "Function Fragment_Stage_0030_Item_00()" in patch
    assert "DispatchWaywardStartEvent()" in patch
    assert "SetLaceyIselaCheckpoint(1.0)" in patch
    assert "If W05_MQ_001P_Wayward_LaceyIselaScene_010" in patch
    assert "If W05_MQ_001P_Wayward_LaceyIselaScene_020" in patch


def test_contract_records_both_stale_source_scene_ids_and_surviving_transition() -> None:
    contract = (
        REPO_ROOT / "bacup" / "docs" / "stub_restoration" / CONTRACT
    ).read_text(encoding="utf-8")

    assert "00405ECB" in contract
    assert "00405ECC" in contract
    assert "00405ED2" in contract
    assert "SCQS.OnStart: 30" in contract
