from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Economy:VendingMachineScript"
RAW_PEX = (
    REPO_ROOT
    / "extracted"
    / "fo76"
    / "scripts"
    / "client"
    / "economy"
    / "vendingmachinescript.pex"
)


def _merged_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    skeleton = decompile_pex(RAW_PEX, fo4_api_compat=True)
    return _merge_script_method_patches(skeleton, patch)


def test_vending_machine_patch_is_scoped_to_caps_machines_and_manages_one_proxy():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "Scriptname" not in patch
    assert "Property" not in patch
    assert 'Game.GetFormFromFile(0x00175087, "SeventySix.esm")' in patch
    assert 'Game.GetFormFromFile(0x001750A5, "SeventySix.esm")' in patch
    assert 'Game.GetFormFromFile(0x001CF4B3, "Fallout4.esm")' in patch
    assert 'Game.GetFormFromFile(0x0005D5E6, "Fallout4.esm")' in patch
    assert "VendingMachineFaction == medicalFaction" in patch
    assert "VendingMachineFaction == ammoFaction" in patch
    assert "GetLinkedRef(proxyLink) as Actor" in patch
    assert "SetLinkedRef(proxy, proxyLink)" in patch
    assert "PlaceAtMe(proxyBase, 1, False, True, False)" in patch
    assert "proxy.AddToFaction(VendingMachineFaction)" in patch
    assert "proxy.ShowBarterMenu()" in patch
    assert "proxy.MoveTo(" not in patch
    assert "proxy.Enable(" not in patch
    assert patch.count("proxy.Disable(False)") == 2
    assert "SetLinkedRef(None, proxyLink)" in patch
    assert "proxy.Delete()" in patch

    expected_members = {
        ("function", "issupportedcapsmachine"),
        ("function", "getvendorproxylink"),
        ("function", "getorcreatevendorproxy"),
        ("event", "onload"),
        ("event", "onactivate"),
        ("event", "onunload"),
    }
    members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert members == expected_members

    merged = _merged_source()
    merged_members = [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected_members:
        assert merged_members.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_vending_machine_patch_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Economy/VendingMachineScript.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
