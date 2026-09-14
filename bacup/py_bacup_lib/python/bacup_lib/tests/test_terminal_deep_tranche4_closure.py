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

SCRIPT_MEMBERS = {
    "Fragments:Terminals:TERM_MoM_Cryptos_Administrat_003694FF": {
        "fragment_terminal_03"
    },
    "Fragments:Terminals:TERM_MoM_Cryptos_Administrat_00369500": {
        "fragment_terminal_05"
    },
    "Fragments:Terminals:TERM_MoM_Cryptos_Administrat_004EB0B9": {
        "fragment_terminal_26"
    },
    "Fragments:Terminals:TERM_LC022_DirectorsOfficeTe_00088A4A": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_MoM_Cryptos_Terminal_000471C5": {
        "setlogin",
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
        "fragment_terminal_05",
    },
    "Fragments:Terminals:TERM_MoM_CryptosTerminalMain_0035801B": {
        "claimpromotion",
        "fragment_terminal_06",
        "fragment_terminal_11",
        "fragment_terminal_12",
    },
    "Fragments:Terminals:TERM_RE_SceneTS06_Terminal_0037E1D8": {
        "setprotectronstage",
        "fragment_terminal_02",
        "fragment_terminal_03",
    },
    "Fragments:Terminals:TERM_E01B_Encryptid_RecallTe_0056F554": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_E01B_Encryptid_RecallTe_00454CBC": {
        "initiaterecall",
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_05",
    },
    "Fragments:Terminals:TERM_BoS_WilsonTerminal_0026B2BD": {
        "fragment_terminal_01"
    },
}

LIVE_BINDING_MANIFEST = {
    "3694FF": (
        "Fragments:Terminals:TERM_MoM_Cryptos_Administrat_003694FF",
        {"Fragment_Terminal_03"},
    ),
    "369500": (
        "Fragments:Terminals:TERM_MoM_Cryptos_Administrat_00369500",
        {"Fragment_Terminal_05"},
    ),
    "4EB0B9": (
        "Fragments:Terminals:TERM_MoM_Cryptos_Administrat_004EB0B9",
        {"Fragment_Terminal_26"},
    ),
    "088A4A": (
        "Fragments:Terminals:TERM_LC022_DirectorsOfficeTe_00088A4A",
        {"Fragment_Terminal_01"},
    ),
    "0471C5": (
        "Fragments:Terminals:TERM_MoM_Cryptos_Terminal_000471C5",
        {
            "Fragment_Terminal_01",
            "Fragment_Terminal_02",
            "Fragment_Terminal_03",
            "Fragment_Terminal_04",
            "Fragment_Terminal_05",
        },
    ),
    "35801B": (
        "Fragments:Terminals:TERM_MoM_CryptosTerminalMain_0035801B",
        {
            "Fragment_Terminal_06",
            "Fragment_Terminal_11",
            "Fragment_Terminal_12",
        },
    ),
    "37E1D8": (
        "Fragments:Terminals:TERM_RE_SceneTS06_Terminal_0037E1D8",
        {"Fragment_Terminal_02", "Fragment_Terminal_03"},
    ),
    "56F554": (
        "Fragments:Terminals:TERM_E01B_Encryptid_RecallTe_0056F554",
        {"Fragment_Terminal_01"},
    ),
    "454CBC": (
        "Fragments:Terminals:TERM_E01B_Encryptid_RecallTe_00454CBC",
        {
            "Fragment_Terminal_01",
            "Fragment_Terminal_02",
            "Fragment_Terminal_05",
        },
    ),
    "26B2BD": (
        "Fragments:Terminals:TERM_BoS_WilsonTerminal_0026B2BD",
        {"Fragment_Terminal_01"},
    ),
}


@pytest.mark.parametrize(("script_name", "expected_members"), SCRIPT_MEMBERS.items())
def test_terminal_deep_tranche4_exact_members_merge_and_compile(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    patch_members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ]
    assert len(patch_members) == len(expected_members)
    assert set(patch_members) == expected_members

    relative_path = _script_relative_path(script_name, ".psc")
    skeleton = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    merged = _merge_script_method_patches(skeleton, patch)
    merged_members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected_members:
        assert merged_members.count(member) == 1
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


def test_terminal_deep_tranche4_semantics_match_live_contracts():
    login = _script_patch_source(
        "Fragments:Terminals:TERM_MoM_Cryptos_Terminal_000471C5"
    )
    promotions = _script_patch_source(
        "Fragments:Terminals:TERM_MoM_CryptosTerminalMain_0035801B"
    )
    protectron = _script_patch_source(
        "Fragments:Terminals:TERM_RE_SceneTS06_Terminal_0037E1D8"
    )
    recall = _script_patch_source(
        "Fragments:Terminals:TERM_E01B_Encryptid_RecallTe_00454CBC"
    )
    assert login is not None
    assert promotions is not None
    assert protectron is not None
    assert recall is not None

    for login_id in range(2, 6):
        assert f"SetLogin(akTerminalRef, {login_id}.0)" in login
    for expected in (
        "ClaimPromotion(2, masterScript.CONST_MoM01_ClaimedPromotion)",
        "ClaimPromotion(3, masterScript.CONST_MoM02_ClaimedPromotion)",
        "ClaimPromotion(8, masterScript.CONST_MoM04_ClaimedPromotion)",
    ):
        assert expected in promotions
    assert "SetProtectronStage(200)" in protectron
    assert "SetProtectronStage(300)" in protectron
    assert "SendStoryEventAndWait(None, player)" in recall
    assert "RemoveItem(P01B_Wolf_RecallKey, 1, true)" in recall


def test_terminal_deep_tranche4_actions_are_guarded_and_local():
    for script_name in SCRIPT_MEMBERS:
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert "Game.GetForm" not in patch
        assert "Debug." not in patch
    for script_name in (
        "Fragments:Terminals:TERM_E01B_Encryptid_RecallTe_00454CBC",
        "Fragments:Terminals:TERM_E01B_Encryptid_RecallTe_0056F554",
    ):
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert "GetItemCount(P01B_Wolf_RecallKey) < 1" in patch
        assert "IsStageDone(100)" in patch


def test_terminal_deep_tranche4_live_binding_manifest_reconciles_with_ledger():
    assert len(LIVE_BINDING_MANIFEST) == 10
    assert sum(len(fragments) for _script, fragments in LIVE_BINDING_MANIFEST.values()) == 19
    assert {script for script, _fragments in LIVE_BINDING_MANIFEST.values()} == set(
        SCRIPT_MEMBERS
    )

    ledger = LEDGER.read_text(encoding="utf-8")
    for form_id, (script_name, live_fragments) in LIVE_BINDING_MANIFEST.items():
        assert script_name.endswith(form_id)
        expected_fragment_members = {
            member for member in SCRIPT_MEMBERS[script_name] if member.startswith("fragment_")
        }
        assert expected_fragment_members == {name.lower() for name in live_fragments}
        basename = script_name.rsplit(":", 1)[-1]
        assert f"`{basename}.psc`" not in ledger
