from __future__ import annotations

import re
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import _iter_top_level_papyrus_members
from creation_lib.pex import decompile_pex


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
PATCH_ROOT = (
    REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"
)
CLIENT_PEX_ROOT = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"
LEDGER = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "root-nonfragment-gap-closure-2026-08-11.md"
)

TRANCHE5_NONDEFECTS = {
    "defaultaliastriggerentershowtutorial.psc": "source PEX",
    "mtr04_getterminalbound.psc": "QP_CorpseFlower",
    "RETriggerScript_NonAppalachia.psc": "four Burn ACTI carriers",
    "ThousandWattDoorScript.psc": "Test_Zachariadis_Lesson2",
}


@pytest.mark.parametrize("relative_path", TRANCHE5_NONDEFECTS)
def test_tranche5_nondefects_have_no_local_member_to_patch(relative_path: str):
    generated = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    generated_members = list(_iter_top_level_papyrus_members(generated.splitlines()))
    assert generated_members == []

    client_pex = relative_path.removesuffix(".psc").lower().replace("/", "\\") + ".pex"
    client_source = decompile_pex(CLIENT_PEX_ROOT / client_pex)
    client_members = list(_iter_top_level_papyrus_members(client_source.splitlines()))
    assert client_members == []
    assert not (PATCH_ROOT / relative_path).exists()


def test_tranche5_reconciliation_is_bounded_in_root_ledger():
    ledger = LEDGER.read_text(encoding="utf-8")
    for relative_path, evidence in TRANCHE5_NONDEFECTS.items():
        assert relative_path.removesuffix(".psc").lower() in ledger.lower()
        assert evidence in ledger

    remaining = ledger.split("## Remaining live hollow rows", 1)[1].split(
        "The four unsupported-online rows", 1
    )[0]
    assert len(re.findall(r"(?m)^\s*\d+ .+\.psc$", remaining)) == 39
    for relative_path in TRANCHE5_NONDEFECTS:
        assert relative_path.lower() not in remaining.lower()


def test_tranche4_delivery_caveats_are_recorded():
    ledger = LEDGER.read_text(encoding="utf-8")
    assert "fresh PSC/PEX output for all three root repairs" in ledger
    assert "five newly augmented terminal declarations" in ledger
    assert "nine accepted quest-fragment" in ledger
