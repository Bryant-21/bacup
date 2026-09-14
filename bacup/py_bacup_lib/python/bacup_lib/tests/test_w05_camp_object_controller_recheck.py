from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "w05-camp-object-controller-recheck-2026-09-01.md"
)

TARGETS = {
    "W05_RE_CampAF02_Quest_Script": {
        "PatientChance",
        "Patient01",
        "Patient02",
        "Patient03",
        "GaveSupply",
        "TookSupply",
        "Reputation_AV_Foundation",
        "Reputation_AV_Crater",
    },
}


def _source(script_name: str) -> str:
    path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    return path.read_text(encoding="utf-8")


@pytest.mark.parametrize(("script_name", "properties"), TARGETS.items())
def test_scoped_sources_remain_exact_declaration_only_shells(
    script_name: str, properties: set[str]
) -> None:
    source = _source(script_name)
    members = list(_iter_top_level_papyrus_members(source.splitlines()))

    assert members == []
    for property_name in properties:
        assert f" Property {property_name} Auto" in source


@pytest.mark.parametrize("script_name", TARGETS)
def test_scoped_sources_have_no_speculative_durable_patch(script_name: str) -> None:
    assert _script_patch_source(script_name) is None


@pytest.mark.parametrize("script_name", TARGETS)
def test_scoped_declaration_shells_native_compile_for_fo4(script_name: str) -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _source(script_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}:\n{diagnostics}"
    assert result.pex_bytes is not None


def test_contract_records_source_pex_and_exact_remaining_dependencies() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    assert contract.count("Decompiled executable members") == 1
    assert "67300D1E4AB42DF27066A562D5D461A2FB25AB79874EE9A5E5A6583350773ECF" in contract
    assert "2DEE10450739D09D9220876B8C08BF39E70A7218BDF85DEE1931B86813AAE621" in contract
    assert "single-player" in contract
    assert "FO76 `QuestInstance` owner" in contract
    assert "activation producer and stage guard" in contract
    assert "`ScavengerRef` is declared in the PEX but absent from the SCEN VMAD" in contract
    assert "superseded" in contract.lower()
