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
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
LEDGER = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "terminal-fragment-remainder-ledger-2026-08.md"
)
SCRIPT_NAME = "Fragments:Terminals:TERM_BoS_TaggerdyTerminal_0034B443"
EXPECTED_MEMBERS = {"fragment_terminal_01"}
LIVE_BINDING_MANIFEST = {
    "34B443": (
        SCRIPT_NAME,
        {"Fragment_Terminal_01", "Fragment_Terminal_05"},
        {"pBoS03", "pArmor_PowerArmor_Ultracite_ArmLeft", "pco_PowerArmor_Ultracite_ArmLeft", "pco_PowerArmor_Ultracite_ArmRight", "pco_PowerArmor_Ultracite_Helmet", "pco_PowerArmor_Ultracite_LegLeft", "pco_PowerArmor_Ultracite_LegRight", "pco_PowerArmor_Ultracite_Torso"},
    )
}


def test_terminal_deep_tranche5_exact_members_merge_and_compile():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    patch_members = {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert patch_members == EXPECTED_MEMBERS

    relative_path = _script_relative_path(SCRIPT_NAME, ".psc")
    skeleton = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative_path),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_terminal_deep_tranche5_taggerdy_stage_contract():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "!pBoS03.IsStageDone(600)" in patch
    assert "pBoS03.SetStage(600)" in patch
    assert "Fragment_Terminal_05" not in patch
    assert "Game.GetForm" not in patch


def test_terminal_deep_tranche5_live_manifest_reconciles_with_patch_and_ledger():
    assert LIVE_BINDING_MANIFEST == {
        "34B443": (
            SCRIPT_NAME,
            {"Fragment_Terminal_01", "Fragment_Terminal_05"},
            {"pBoS03", "pArmor_PowerArmor_Ultracite_ArmLeft", "pco_PowerArmor_Ultracite_ArmLeft", "pco_PowerArmor_Ultracite_ArmRight", "pco_PowerArmor_Ultracite_Helmet", "pco_PowerArmor_Ultracite_LegLeft", "pco_PowerArmor_Ultracite_LegRight", "pco_PowerArmor_Ultracite_Torso"},
        )
    }
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    patch_members = {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert patch_members == EXPECTED_MEMBERS
    assert EXPECTED_MEMBERS < {
        member.lower()
        for member in LIVE_BINDING_MANIFEST["34B443"][1]
    }
    ledger = LEDGER.read_text(encoding="utf-8")
    assert "`TERM_BoS_TaggerdyTerminal_0034B443.psc`" not in ledger
