from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
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

ACCEPTED_MEMBERS = {
    "Fragments:Terminals:TERM_nativeRobotTerminalSubM_002C506F": {
        "setlinkedrobotsenabled",
        "removelinkedrobottargetingrestrictions",
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_04",
    },
    "Fragments:Terminals:TERM_SFL02_Track_VertibotTer_00184A03": {
        "fragment_terminal_05"
    },
}


@pytest.mark.parametrize(("script_name", "expected_members"), ACCEPTED_MEMBERS.items())
def test_declaration_unlocked_terminal_members_merge_and_augmented_source_compiles(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    patch_members = {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert patch_members == expected_members

    relative = _script_relative_path(script_name, ".psc")
    current = (SOURCE_ROOT / relative).read_text(encoding="utf-8")
    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, current)
    merged = _merge_script_method_patches(augmented, patch)
    assert _merge_script_method_patches(merged, patch) == merged

    base = _fo4_base_source()
    if base is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_native_robot_uses_exact_protectron_link_and_counts_new_shutdowns_once():
    patch = _script_patch_source(
        "Fragments:Terminals:TERM_nativeRobotTerminalSubM_002C506F"
    )
    assert patch is not None
    assert "GetLinkedRefArray(LinkTerminalProtectron)" in patch
    assert "Bool wasDisabled = robot.IsUnconscious()" in patch
    assert "If !wasDisabled && player != None" in patch
    assert "player.ModValue(MiscStatRobotHasBeenDisabled, 1.0)" in patch


def test_sfl02_records_exact_global_and_player_discovery_state():
    patch = _script_patch_source(
        "Fragments:Terminals:TERM_SFL02_Track_VertibotTer_00184A03"
    )
    assert patch is not None
    assert "LCP_BoSZ01.SetValue(1.0)" in patch
    assert "playerRef.SetValue(pBoSz01_PlayerKACacheDepot, 1.0)" in patch


def test_unresolved_transaction_and_vault_door_contracts_remain_unpatched():
    assert _script_patch_source(
        "Fragments:Terminals:TERM_CB02_Vending_Terminal_T_0051AA03"
    ) is None
    assert _script_patch_source(
        "Fragments:Terminals:TERM_V94_Access_Terminal_002FB273"
    ) is None
    ledger = LEDGER.read_text(encoding="utf-8")
    assert "`TERM_CB02_Vending_Terminal_T_0051AA03.psc`" in ledger
    assert "`term_v94_access_terminal_002fb273.psc`" in ledger
