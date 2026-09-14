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
LEDGER_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "contracts" / "quest-fragment-production-tranche-2026-08-11.csv"

MANIFEST = {
    "Fragments:Quests:QF_MTNS06_Uranium_Misc_000364D1": ("fragment_stage_0200_item_00", 2),
    "Fragments:Quests:QF_MTRZ01_Lost_00053E56": ("fragment_stage_0300_item_00", 3),
    "Fragments:Quests:QF_MTRZ01_Trigger_00161F92": ("fragment_stage_0300_item_00", 4),
    "Fragments:Quests:QF_SQ_SmallFissureSpawner_004FD185": ("fragment_stage_9999_item_00", 2),
}


def _members(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged(script_name: str) -> tuple[str, Path]:
    patch = _script_patch_source(script_name)
    assert patch is not None
    relative = _script_relative_path(script_name, ".psc")
    source = (SOURCE_ROOT / relative).read_text(encoding="utf-8")
    return _merge_script_method_patches(source, patch), relative


def test_deep_tranche7_manifest_and_ledger_are_exact() -> None:
    assert len(MANIFEST) == 4
    assert all(1 < live_count for _member, live_count in MANIFEST.values())
    with LEDGER_PATH.open(encoding="utf-8", newline="") as stream:
        rows = list(csv.DictReader(stream))
    tranche_rows = [
        row for row in rows if row["evidence_gate"].startswith("tranche-7 partial ")
    ]
    assert len(tranche_rows) == 4
    assert all(row["disposition"] == "partial-local-quest-action" for row in tranche_rows)
    assert sum(
        row["disposition"] == "blocked-insufficient-body-evidence" for row in rows
    ) == 236


@pytest.mark.parametrize(("script_name", "contract"), MANIFEST.items())
def test_deep_tranche7_member_only_stop_merge_is_idempotent(
    script_name: str, contract: tuple[str, int]
) -> None:
    expected_member, _live_count = contract
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert _members(patch) == [expected_member]
    assert patch.count("Stop()") == 1
    merged, _relative = _merged(script_name)
    assert sum(name == expected_member for name in _members(merged)) == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", MANIFEST)
def test_deep_tranche7_merged_full_source_native_compiles(script_name: str) -> None:
    merged, relative = _merged(script_name)
    base = _fo4_base_source()
    assert base is not None
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}\n{diagnostics}"
    assert result.pex_bytes is not None
