from __future__ import annotations

import csv
from pathlib import Path

from bacup_lib.workflows.unified import _script_patch_source


REPO_ROOT = Path(__file__).resolve().parents[5]
STATUS_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
CONTRACT = "contracts/w05-community-package-client-body-closure.md"

BODYLESS_PACKAGE_CASES = (
    "Fragments:Packages:PF_W05_Community_Treehouse_E_00555B59",
)
BED_AND_BREAKFAST_SUCCESSORS = (
    "Fragments:Packages:PF_W05_Community_BB_TravelTo_0054E4F9",
    "Fragments:Packages:PF_W05_Community_BB_TravelTo_0054F9B2",
)


def test_community_bodyless_packages_are_closed_without_invented_patches():
    with STATUS_PATH.open(encoding="utf-8", newline="") as status_file:
        rows = {row["script_name"].lower(): row for row in csv.DictReader(status_file)}

    for script_name in BODYLESS_PACKAGE_CASES:
        row = rows[script_name.lower()]
        assert row["terminal_state"] == "non-defect"
        assert row["evidence"] == CONTRACT
        assert _script_patch_source(script_name) is None

    successor_contract = "contracts/w05-community-bed-and-breakfast.md"
    for script_name in BED_AND_BREAKFAST_SUCCESSORS:
        row = rows[script_name.lower()]
        assert row["terminal_state"] == "patched"
        assert row["evidence"] == successor_contract
        assert _script_patch_source(script_name) is not None


def test_community_package_closure_contract_names_every_row():
    contract = (STATUS_PATH.parent / CONTRACT).read_text(encoding="utf-8").lower()

    original_cases = BODYLESS_PACKAGE_CASES + BED_AND_BREAKFAST_SUCCESSORS
    assert len(original_cases) == 3
    for script_name in original_cases:
        form_id = script_name.rsplit("_", 1)[-1].lstrip("0").lower()
        assert form_id in contract
