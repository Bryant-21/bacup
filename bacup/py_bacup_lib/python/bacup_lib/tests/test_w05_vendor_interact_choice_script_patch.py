from __future__ import annotations

import csv
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
SCRIPT_NAME = "VendorInteractChoiceScript"
STATUS_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
VMAD_CLOSURE_CONTRACT = "contracts/w05-vendor-perk-vmad-closure.md"
SOURCE_PEX = (
    REPO_ROOT
    / "extracted"
    / "fo76"
    / "scripts"
    / "client"
    / "vendorinteractchoicescript.pex"
)


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source() -> tuple[str, str]:
    assert SOURCE_PEX.is_file(), SOURCE_PEX
    skeleton = decompile_pex(SOURCE_PEX, fo4_api_compat=True)
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch, _merge_script_method_patches(skeleton, patch)


def test_w05_vendor_helper_merges_once_and_gates_online_economies():
    patch, merged = _merged_source()

    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )

    expected_signature = (
        "Function TriggerVendorInteraction(Actor currentPlayer, ObjectReference vendor)"
    )
    body = _member_body(patch, "triggervendorinteraction")
    assert body.splitlines()[0] == expected_signature
    assert _member_body(merged, "triggervendorinteraction") == body
    assert merged.lower().count("function triggervendorinteraction(") == 1
    assert _merge_script_method_patches(merged, patch) == merged

    assert "currentPlayer != Game.GetPlayer()" in body
    assert "vendor == None" in body
    assert "Utility.IsInMenuMode()" in body
    assert "vendor as Actor" in body
    assert "vendor.GetBaseObject()" in body

    blocked_keywords = ("0x004F5632", "0x005A11A1", "0x0065D4C8")
    ordinary_keywords = ("0x005614E6", "0x0059276B", "0x00593DCF")
    for form_id in blocked_keywords + ordinary_keywords:
        assert (
            body.count(f'Game.GetFormFromFile({form_id}, "SeventySix.esm") as Keyword')
            == 1
        )

    blocked_gate = "If vendorBase.HasKeyword(genericVendorKeyword)"
    ordinary_gate = "If vendorBase.HasKeyword(craterVendorKeyword)"
    assert body.index(blocked_gate) < body.index(ordinary_gate)
    assert body.index(ordinary_gate) < body.index("Utility.Wait(0.2)")
    assert body.index("Utility.Wait(0.2)") < body.index("vendorActor.ShowBarterMenu()")
    assert body.count("ShowBarterMenu()") == 1

    for unsupported_effect in (
        "AddItem(",
        "RemoveItem(",
        "SetValue(",
        "ModValue(",
        "SetValueInt(",
    ):
        assert unsupported_effect not in body


def test_w05_vendor_helper_native_compiles_for_fo4():
    _patch, merged = _merged_source()
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="VendorInteractChoiceScript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_w05_vendor_helper_record_dependency_is_closed() -> None:
    with STATUS_PATH.open(encoding="utf-8", newline="") as stream:
        row = next(
            row
            for row in csv.DictReader(stream)
            if row["script_name"] == SCRIPT_NAME
        )

    assert row["terminal_state"] == "patched"
    assert row["evidence"] == VMAD_CLOSURE_CONTRACT
