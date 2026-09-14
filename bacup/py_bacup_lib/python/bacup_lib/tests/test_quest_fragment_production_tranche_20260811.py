from __future__ import annotations

import csv
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
PATCH_ROOT = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "python"
    / "bacup_lib"
    / "script_patches"
    / "Fragments"
    / "Quests"
)
LEDGER_PATH = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "quest-fragment-production-tranche-2026-08-11.csv"
)

PATCH_MEMBERS = {
    "Fragments:Quests:QF_ArcadeNukaZapperRace_Obje_006751AF": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
    },
    "Fragments:Quests:QF_E01C_Tales_PennyDialogueQ_003EE22A": {
        "fragment_stage_0100_item_00"
    },
    "Fragments:Quests:QF_P01C_Bucket_Corpse_00478390": {
        "fragment_stage_0100_item_00"
    },
    "Fragments:Quests:QF_RE_TravelCMB01_00529E17": {
        "fragment_stage_1000_item_00"
    },
    "Fragments:Quests:QF_RE_TravelCMB02_00529E18": {
        "fragment_stage_1000_item_00"
    },
    "Fragments:Quests:QF_RE_TravelCMB03_00529E19": {
        "fragment_stage_1000_item_00"
    },
    "Fragments:Quests:QF_RE_TravelCMB04_00529E1A": {
        "fragment_stage_1000_item_00"
    },
    "Fragments:Quests:QF_RE_TravelCMB05_00529E1B": {
        "fragment_stage_1000_item_00"
    },
    "Fragments:Quests:QF_RE_TravelCMB11_005204D5": {
        "fragment_stage_1000_item_00"
    },
    "Fragments:Quests:QF_RE_TravelMP02_0035EC06": {
        "fragment_stage_1000_item_00"
    },
    "Fragments:Quests:QF_RE_TravelTS02_0046985F": {
        "fragment_stage_0100_item_00"
    },
    "Fragments:Quests:QF_SHELS01_InvestigateCorpse_005DB4DA": {
        "fragment_stage_0110_item_00"
    },
    "Fragments:Quests:QF_SHELS01_PosterMiscObjecti_005DB35C": {
        "fragment_stage_0110_item_00"
    },
}

REARM_SCRIPT_NAMES = set()
for patch_path in PATCH_ROOT.glob("QF_RE*.psc"):
    patch_source = patch_path.read_text(encoding="utf-8")
    if (
        "Function Fragment_Stage_1000_Item_00()" in patch_source
        and "Alias_TRIGGER.GetReference()" in patch_source
        and "as RETriggerScript" in patch_source
        and "triggerScript.ReArmTrigger()" in patch_source
    ):
        script_name = f"Fragments:Quests:{patch_path.stem}"
        REARM_SCRIPT_NAMES.add(script_name)
        PATCH_MEMBERS[script_name] = {"fragment_stage_1000_item_00"}

with LEDGER_PATH.open(encoding="utf-8", newline="") as stream:
    _deep_tranche2_script_names = {
        f"Fragments:Quests:{Path(row['generated_file']).stem}".casefold()
        for row in csv.DictReader(stream)
        if row["evidence_gate"].startswith("tranche-2 ")
    }

REARM_SCRIPT_NAMES = {
    script_name
    for script_name in REARM_SCRIPT_NAMES
    if script_name.casefold() not in _deep_tranche2_script_names
}
PATCH_MEMBERS = {
    script_name: members
    for script_name, members in PATCH_MEMBERS.items()
    if script_name.casefold() not in _deep_tranche2_script_names
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_production_tranche_contains_at_least_one_hundred_exact_patches():
    assert len(PATCH_MEMBERS) >= 100


def test_production_tranche_ledger_covers_every_baseline_gap_once():
    with LEDGER_PATH.open(encoding="utf-8", newline="") as stream:
        rows = list(csv.DictReader(stream))

    assert len(rows) == 1168
    assert len({row["generated_file"].casefold() for row in rows}) == len(rows)
    assert all(
        (SOURCE_ROOT / "Fragments" / "Quests" / row["generated_file"]).is_file()
        for row in rows
    )
    assert all(row["disposition"] and row["evidence_gate"] for row in rows)

    restored_rows = [
        row
        for row in rows
        if row["disposition"].startswith(("patched-", "partial-"))
        and not row["evidence_gate"].startswith(
            (
                "tranche-2 ",
                "tranche-3 ",
                "tranche-4 ",
                "tranche-5 ",
                "tranche-6 ",
                "tranche-7 ",
            )
        )
    ]
    assert len(restored_rows) == len(PATCH_MEMBERS)
    assert {
        f"Fragments:Quests:{Path(row['generated_file']).stem}".casefold()
        for row in restored_rows
    } == {script_name.casefold() for script_name in PATCH_MEMBERS}

    trigger_rows = [
        row
        for row in rows
        if row["disposition"] in {"patched-local-trigger", "partial-local-trigger"}
        and not row["evidence_gate"].startswith(("tranche-2 ", "tranche-3 "))
    ]
    assert len(trigger_rows) == 223
    assert sum(row["disposition"] == "patched-local-trigger" for row in trigger_rows) == 166
    assert sum(row["disposition"] == "partial-local-trigger" for row in trigger_rows) == 57
    assert next(
        row
        for row in rows
        if row["generated_file"] == "QF_RE_ObjectDRE05GQ_000187DD.psc"
    )["disposition"] == "intentional-unused"


@pytest.mark.parametrize("script_name", sorted(REARM_SCRIPT_NAMES))
def test_random_encounter_shutdown_rearms_reusable_trigger(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert "Alias_TRIGGER.GetReference() as RETriggerScript" in patch
    assert "triggerScript.ReArmTrigger()" in patch
    assert ".Disable()" not in patch


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_production_quest_fragment_patch_merges_exact_members(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )

    merged = _merged_source(script_name)
    merged_names = _member_names(merged)
    for member in members:
        assert merged_names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", PATCH_MEMBERS)
def test_production_quest_fragment_full_source_native_compiles(
    script_name: str, tmp_path: Path
):
    merged = _merged_source(script_name)
    merged_source_root = tmp_path / "Scripts" / "Source" / "User"
    source_path = merged_source_root / _script_relative_path(script_name, ".psc")
    source_path.parent.mkdir(parents=True, exist_ok=True)
    source_path.write_text(merged, encoding="utf-8")

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(merged_source_root), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}\n{diagnostics}"
    assert result.pex_bytes is not None
