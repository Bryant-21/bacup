from __future__ import annotations

import csv
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
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
    / "nonquest-fragment-row-ledger-2026-08-11.csv"
)
PATCHES = {
    "Fragments:Packages:PF_MTR10_Battle_Robot01RETRE_00342A8D": (
        "Function Fragment_End(Actor akActor)",
        "MTR10_Battle.SetStage(160)",
    ),
    "Fragments:Perks:PRKF_TWZ09AerosilzerKick_002FD347": (
        "Function Fragment_Entry_00(ObjectReference akTargetRef, Actor akActor)",
        "akActor == Game.GetPlayer()",
        "TWZ09.SetStage(10)",
    ),
}
PREVIOUS_PRODUCER_OMISSIONS = {
    "Fragments:Packages:PF_RD01_Enc04_EyebotPackage_0078BEC3": (
        "RefCollectionAlias Property Generators Auto Mandatory",
    ),
    "Fragments:Scenes:SF_V94_1_0015FB0A": (
        "Keyword Property V94AccessQuestKeyword Auto Mandatory",
        "Scene Property V94Radio_1 Auto Mandatory",
    ),
    "Fragments:Scenes:SF_V94_3_0015FB12": (
        "Keyword Property V94AccessQuestKeyword Auto Mandatory",
        "Scene Property V94Radio_3 Auto Mandatory",
    ),
}


def _merged(script_name: str) -> tuple[str, Path]:
    relative = _script_relative_path(script_name, ".psc")
    source = (SOURCE_ROOT / relative).read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(source, patch), relative


@pytest.mark.parametrize(("script_name", "snippets"), PATCHES.items())
def test_tranche6_deterministic_members_merge_idempotently(
    script_name: str, snippets: tuple[str, ...]
) -> None:
    merged, _ = _merged(script_name)
    for snippet in snippets:
        assert snippet in merged
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", PATCHES)
def test_tranche6_deterministic_members_full_source_native_compile(
    script_name: str,
) -> None:
    base = _fo4_base_source()
    if base is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    merged, relative = _merged(script_name)
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


@pytest.mark.parametrize(
    ("script_name", "declarations"), PREVIOUS_PRODUCER_OMISSIONS.items()
)
def test_tranche6_nonquest_declaration_rescan_has_no_remaining_known_omission(
    script_name: str, declarations: tuple[str, ...]
) -> None:
    relative = _script_relative_path(script_name, ".psc")
    source = (SOURCE_ROOT / relative).read_text(encoding="utf-8")
    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, source)
    for declaration in declarations:
        assert augmented.count(declaration) == 1
    assert _augment_fo76_to_fo4_script_skeleton(script_name, augmented) == augmented


def test_tranche6_row_ledger_records_only_the_two_proven_new_actions() -> None:
    with LEDGER.open(encoding="utf-8", newline="") as stream:
        rows = list(csv.DictReader(stream))
    indexed = {(row["family"], row["record_id"]): row for row in rows}
    assert indexed[("Packages", "342A8D")]["disposition"] == "patched"
    assert indexed[("Perks", "2FD347")]["disposition"] == "patched"
    assert sum(
        row["family"] == "Packages" and row["disposition"] == "evidence-blocked"
        for row in rows
    ) == 97
    assert sum(
        row["family"] == "Scenes" and row["disposition"] == "evidence-blocked"
        for row in rows
    ) == 94
    assert sum(
        row["family"] == "Perks" and row["disposition"] == "evidence-blocked"
        for row in rows
    ) == 0
