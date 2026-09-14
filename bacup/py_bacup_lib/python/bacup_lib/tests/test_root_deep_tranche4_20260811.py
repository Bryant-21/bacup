from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from tools.stub_evidence.live_vmad_probe import probe
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
PATCH_ROOT = (
    REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"
)
PLUGIN = Path(
    os.environ.get(
        "B21_TEST_SEVENTYSIX_ESM",
        REPO_ROOT / "mods" / "SeventySix" / "SeventySix.esm",
    )
)
LEDGER = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "root-nonfragment-gap-closure-2026-08-11.md"
)
REPAIRS = (
    "EN06_PresEquipmentTerminalScript.psc",
    "MoMCryptosTerminalScript.psc",
    "RS03_Balance_TerminalScript.psc",
)
REPAIR_MANIFEST = {
    "EN06_PresEquipmentTerminalScript.psc": {
        "member": ("event", "onactivate"),
        "live_owner": "TERM 52BDE0 EN06_PresidentialVendorTerminal",
    },
    "MoMCryptosTerminalScript.psc": {
        "member": ("event", "onmenuitemrun"),
        "live_owner": "TERM 0471C5 MoM_CryptosTerminal",
    },
    "RS03_Balance_TerminalScript.psc": {
        "member": ("event", "onmenuitemrun"),
        "live_owner": "TERM 11B1F3 FF05_Balance_TerminalSubEnviroMonitoring",
    },
}


def _merged(relative_path: str) -> str:
    skeleton = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    patch = _script_patch_source(relative_path.removesuffix(".psc"))
    assert patch is not None
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


@pytest.mark.parametrize("relative_path", REPAIRS)
def test_tranche4_patch_has_exactly_one_expected_top_level_member(relative_path: str):
    patch = (PATCH_ROOT / relative_path).read_text(encoding="utf-8")
    members = [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ]
    assert members == [REPAIR_MANIFEST[relative_path]["member"]]

    merged_members = [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            _merged(relative_path).splitlines()
        )
    ]
    assert merged_members.count(REPAIR_MANIFEST[relative_path]["member"]) == 1


def test_tranche4_manifest_matches_root_ledger():
    assert set(REPAIR_MANIFEST) == set(REPAIRS)
    ledger = LEDGER.read_text(encoding="utf-8")
    for relative_path, contract in REPAIR_MANIFEST.items():
        assert relative_path.removesuffix(".psc") in ledger
        assert contract["live_owner"] in ledger


def test_tranche4_manifest_matches_live_vmad():
    script_names = [path.removesuffix(".psc") for path in REPAIR_MANIFEST]
    assert probe(PLUGIN, script_names) == dict.fromkeys(script_names, 1)


@pytest.mark.parametrize("relative_path", REPAIRS)
def test_tranche4_root_repairs_merge_idempotently_and_compile(relative_path: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        _merged(relative_path),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=relative_path,
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_cryptos_terminal_maps_exact_login_items_to_bound_user_tokens():
    merged = _merged("MoMCryptosTerminalScript.psc")
    assert "auiMenuItemID == 2" in merged and "userIndex = 1" in merged
    assert "auiMenuItemID == 3" in merged and "userIndex = 0" in merged
    assert "auiMenuItemID == 4" in merged and "userIndex = 2" in merged
    assert "auiMenuItemID == 5" in merged and "userIndex = 3" in merged
    assert "selectedUser.UserIDWithBar" in merged
    assert "selectedUser.UserIDWithoutBar" in merged
    assert "selectedUser.UserIDNoSpaces" in merged


def test_balance_terminal_consumes_only_the_selected_bound_data_holotape():
    merged = _merged("RS03_Balance_TerminalScript.psc")
    assert "dataHolotape = RS03_BalanceWaterDataHolotape" in merged
    assert "dataHolotape = RS03_BalanceSoilDataHolotape" in merged
    assert "dataHolotape = RS03_BalanceAirDataHolotape" in merged
    assert "playerRef.GetItemCount(dataHolotape) > 0" in merged
    assert "playerRef.RemoveItem(dataHolotape, 1, True)" in merged
    assert "RS03_Balance_MissingDataMessage.Show()" in merged


def test_presidential_vendor_refreshes_its_bound_seal_confirmation_on_activate():
    merged = _merged("EN06_PresEquipmentTerminalScript.psc")
    assert "akActionRef == playerRef" in merged
    assert "playerRef.GetItemCount(EN06_PresidentialSeal) > 0" in merged
