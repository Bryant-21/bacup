from __future__ import annotations

import csv
from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
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
    / "topicinfo-fragment-gap-ledger-2026-08-11.csv"
)
SCRIPT_NAME = "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F819"


def test_topicinfo_deep_tranche6_attack_branch_merges_and_compiles_full_source():
    base_name = SCRIPT_NAME.rsplit(":", 1)[-1]
    skeleton = (
        SOURCE_ROOT / "fragments" / "topicinfos" / f"{base_name}.psc"
    ).read_text(encoding="utf-8-sig")
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ] == [("function", "fragment_end")]
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{base_name}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_topicinfo_deep_tranche6_removes_shared_faction_before_combat():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    removal = "SettlerAllies.RemoveRef(player)"
    combat = "settler.StartCombat(player)"
    assert removal in patch
    assert combat in patch
    assert patch.index(removal) < patch.index(combat)
    assert "AddRef" not in patch
    assert ".Kill(" not in patch


def test_topicinfo_deep_tranche6_ledger_matches_exact_live_contract():
    with LEDGER.open(encoding="utf-8", newline="") as stream:
        rows = {
            row["script"].casefold(): row
            for row in csv.DictReader(stream)
            if row["disposition"] == "patched-deterministic-tranche6"
        }
    assert rows.keys() == {SCRIPT_NAME.rsplit(":", 1)[-1].casefold()}
    row = next(iter(rows.values()))
    assert row["runtime_form_id"] == "0055F819"
    assert row["exact_live_binding"] == "true"
    assert row["fragments"].casefold() == "fragment_end"
    assert row["patch_members"].casefold() == "fragment_end"
    assert "[attack]" in row["evidence"].casefold()
    assert "removeref" in row["evidence"].casefold()
    assert "startcombat" in row["evidence"].casefold()


def test_topicinfo_deep_tranche6_exhaustive_remainder_count():
    with LEDGER.open(encoding="utf-8", newline="") as stream:
        deferred = [
            row
            for row in csv.DictReader(stream)
            if row["disposition"] == "evidence-deferred"
        ]
    assert len(deferred) == 8
