from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_PEX_ROOT = (
    REPO_ROOT / "extracted" / "fo76" / "scripts" / "client" / "fragments" / "perks"
)
VENDOR_HELPER_PEX = (
    REPO_ROOT
    / "extracted"
    / "fo76"
    / "scripts"
    / "client"
    / "vendorinteractchoicescript.pex"
)

PERK_CASES = {
    "Fragments:Perks:PRKF_W05_Crater_PlayerVendor_005614E5": (
        "prkf_w05_crater_playervendor_005614e5.pex",
        ("fragment_entry_00",),
    ),
    "Fragments:Perks:PRKF_W05_Foundation_PlayerVe_0059276C": (
        "prkf_w05_foundation_playerve_0059276c.pex",
        ("fragment_entry_00",),
    ),
    "Fragments:Perks:PRKF_W05_PlayerGoldVendorInt_005A11A2": (
        "prkf_w05_playergoldvendorint_005a11a2.pex",
        ("fragment_entry_00", "fragment_entry_02"),
    ),
    "Fragments:Perks:PRKF_W05_Wayward_PlayerVendo_00593DD5": (
        "prkf_w05_wayward_playervendo_00593dd5.pex",
        ("fragment_entry_00",),
    ),
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _source_skeleton(source_pex_name: str) -> str:
    source_pex = SOURCE_PEX_ROOT / source_pex_name
    assert source_pex.is_file(), source_pex
    return decompile_pex(source_pex, fo4_api_compat=True)


@pytest.mark.parametrize(("script_name", "case"), PERK_CASES.items())
def test_vendor_entry_fragments_merge_once_and_preserve_delegated_economy(
    script_name: str, case: tuple[str, tuple[str, ...]]
):
    source_pex_name, expected_members = case
    skeleton = _source_skeleton(source_pex_name)
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )
    assert "self as perk as vendorinteractchoicescript." in skeleton.lower()

    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    assert "self as perk as vendorinteractchoicescript." not in merged.lower()

    for member_name in expected_members:
        assert _member_names(patch).count(member_name) == 1
        assert _member_names(merged).count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)

        body = _member_body(patch, member_name)
        assert "akActor == Game.GetPlayer()" in body
        assert (
            body.count(
                "VendorInteractChoiceScript vendorChoice = (Self as Form) as VendorInteractChoiceScript"
            )
            == 1
        )
        assert body.count("If vendorChoice") == 1
        assert (
            body.count(
                "vendorChoice.TriggerVendorInteraction(akActor, akTargetRef)"
            )
            == 1
        )
        assert body.index("If vendorChoice") < body.index(
            "vendorChoice.TriggerVendorInteraction(akActor, akTargetRef)"
        )
        assert "Self as VendorInteractChoiceScript" not in body

    assert set(_member_names(patch)) == set(expected_members)
    assert "ShowBarterMenu" not in patch
    assert "AddItem(" not in patch
    assert "RemoveItem(" not in patch


@pytest.mark.parametrize(("script_name", "case"), PERK_CASES.items())
def test_vendor_entry_fragments_native_compile_for_fo4(
    script_name: str,
    case: tuple[str, tuple[str, ...]],
    tmp_path: Path,
):
    source_pex_name, _expected_members = case
    skeleton = _source_skeleton(source_pex_name)
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merge_script_method_patches(skeleton, patch)

    assert VENDOR_HELPER_PEX.is_file(), VENDOR_HELPER_PEX
    helper_source = decompile_pex(VENDOR_HELPER_PEX, fo4_api_compat=True)
    (tmp_path / "VendorInteractChoiceScript.psc").write_text(
        helper_source, encoding="utf-8"
    )

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(tmp_path), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
