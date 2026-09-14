from __future__ import annotations

from pathlib import Path
import re

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
LEDGER = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "contracts" / "terminal-fragment-remainder-ledger-2026-08.md"

LIVE_MANIFEST = {
    "05A2D4": (
        "Fragments:Terminals:TERM_V94_Atrium_CommunityC_0005A2D4_2",
        {"fragment_terminal_05"},
        {"V94", "V94_HolotapeCommunityCouncil"},
    ),
    "05A2D9": (
        "Fragments:Terminals:TERM_V94_Eng_GECKMonitoringS_0005A2D9",
        {"fragment_terminal_04"},
        {"V94", "V94_HolotapeGECK"},
    ),
}
CUT_NONDEFECTS = (
    "Fragments:Terminals:TERM_V63_Security_MissionTer_00324213",
    "Fragments:Terminals:TERM_V96_Security_MissionTer_00324187",
    "Fragments:Terminals:TERM_v96_FinalTerminal_0032CDE3",
)


@pytest.mark.parametrize(
    ("script_name", "expected_members"),
    [(script, members) for script, members, _properties in LIVE_MANIFEST.values()],
)
def test_terminal_deep_tranche6_exact_members_merge_and_compile(
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

    relative_path = _script_relative_path(script_name, ".psc")
    skeleton = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None
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


@pytest.mark.parametrize(
    ("script_name", "holotape", "member"),
    [
        (
            "Fragments:Terminals:TERM_V94_Atrium_CommunityC_0005A2D4_2",
            "V94_HolotapeCommunityCouncil",
            "Fragment_Terminal_05",
        ),
        (
            "Fragments:Terminals:TERM_V94_Eng_GECKMonitoringS_0005A2D9",
            "V94_HolotapeGECK",
            "Fragment_Terminal_04",
        ),
    ],
)
def test_terminal_deep_tranche6_holotape_exports_are_local_and_idempotent(
    script_name: str, holotape: str, member: str
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert member in patch
    assert f"playerRef.GetItemCount({holotape}) == 0" in patch
    assert f"playerRef.AddItem({holotape}, 1, False)" in patch
    assert "RemoveItem" not in patch


def test_terminal_deep_tranche6_manifest_and_remainder_reconcile():
    assert set(LIVE_MANIFEST) == {"05A2D4", "05A2D9"}
    assert sum(len(members) for _script, members, _props in LIVE_MANIFEST.values()) == 2
    ledger = LEDGER.read_text(encoding="utf-8")
    for form_id, (script_name, expected_members, expected_properties) in LIVE_MANIFEST.items():
        assert form_id in script_name.upper()
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert {
            name
            for _kind, name, _start, _end in _iter_top_level_papyrus_members(
                patch.splitlines()
            )
        } == expected_members
        skeleton = (SOURCE_ROOT / _script_relative_path(script_name, ".psc")).read_text(encoding="utf-8")
        for property_name in expected_properties:
            assert f" Property {property_name} Auto" in skeleton
        assert f"`{script_name.rsplit(':', 1)[-1]}.psc`" not in ledger

    rows = re.findall(r"^\| `[^`]+` \| ([a-z0-9-]+) \|$", ledger, re.MULTILINE)
    assert len(rows) == 261
    assert {disposition: rows.count(disposition) for disposition in set(rows)} == {
        "bound-property-contract-blocked": 34,
        "display-service-blocked": 11,
        "live-binding-type-mismatch-blocked": 1,
        "live-topology-or-orphan-trace-blocked": 34,
        "raid-or-newer-controller-blocked": 52,
        "service-transaction-blocked": 41,
        "source-identity-blocked": 71,
        "test-content-nondefect": 17,
    }


def test_terminal_deep_tranche6_cut_records_remain_intentionally_unpatched():
    ledger = LEDGER.read_text(encoding="utf-8")
    for script_name in CUT_NONDEFECTS:
        assert _script_patch_source(script_name) is None
        basename = script_name.rsplit(":", 1)[-1]
        assert f"`{basename}.psc` | test-content-nondefect" in ledger
