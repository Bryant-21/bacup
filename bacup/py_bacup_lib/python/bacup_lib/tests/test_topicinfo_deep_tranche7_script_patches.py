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
SCRIPT_NAME = "Fragments:TopicInfos:TIF_MTNM01_Mayhem_00098B49"
EXPECTED_DEFERRED = {
    "tif_debug_questmaster_lvc_0040bdf5",
    "tif_debug_questmaster_lvc_00566bd5",
    "tif_debugcorriequest_00553c62",
    "tif_dialogue_e07b_invaders_h_00629dd3",
    "tif_dialogue_mtns03_questgiv_00027df1",
    "tif_e05_caravan_dialogue_00584d80",
    "tif_w05_re_scene_jp02_0055f829",
    "tif_w05_re_scene_jp02_0055f833",
}


def _patch() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def test_topicinfo_deep_tranche7_recipe_stage_merges_and_compiles_full_source():
    base_name = SCRIPT_NAME.rsplit(":", 1)[-1]
    skeleton = (
        SOURCE_ROOT / "fragments" / "topicinfos" / f"{base_name}.psc"
    ).read_text(encoding="utf-8-sig")
    patch = _patch()
    assert [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ] == [("function", "fragment_begin")]
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


def test_topicinfo_deep_tranche7_sets_exact_recipe_unlock_stage_without_item_grant():
    patch = _patch()
    operations = (
        "GetOwningQuest()",
        "owningQuest.IsRunning()",
        "!owningQuest.IsStageDone(400)",
        "owningQuest.SetStage(400)",
    )
    offsets = [patch.index(operation) for operation in operations]
    assert offsets == sorted(offsets)
    assert "AddItem" not in patch
    assert "RemoveItem" not in patch
    assert "Game.GetForm" not in patch
    assert "450366" not in patch


def test_topicinfo_deep_tranche7_ledger_matches_record_backed_contract():
    with LEDGER.open(encoding="utf-8", newline="") as stream:
        rows = {
            row["script"].casefold(): row
            for row in csv.DictReader(stream)
            if row["disposition"] == "patched-record-backed-tranche7"
        }
    assert rows.keys() == {SCRIPT_NAME.rsplit(":", 1)[-1].casefold()}
    row = next(iter(rows.values()))
    assert row["runtime_form_id"] == "00098B49"
    assert row["exact_live_binding"] == "true"
    assert row["fragments"].casefold() == "fragment_begin"
    assert row["patch_members"].casefold() == "fragment_begin"
    assert "stage 400" in row["evidence"].casefold()
    assert "cobj 32da8c" in row["evidence"].casefold()
    assert "tales" in row["evidence"].casefold()


def test_topicinfo_deep_tranche7_exhaustive_remainder_count():
    with LEDGER.open(encoding="utf-8", newline="") as stream:
        deferred = {
            row["script"].casefold()
            for row in csv.DictReader(stream)
            if row["disposition"] == "evidence-deferred"
        }
    assert deferred == EXPECTED_DEFERRED
