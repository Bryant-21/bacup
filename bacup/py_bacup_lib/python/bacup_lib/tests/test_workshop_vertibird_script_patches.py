from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_CASES = {
    "WorkshopVertibirdGrenadeScript": REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "WorkshopVertibirdGrenadeScript.psc",
    "Fragments:Quests:QF_SQ_WorkshopVertibirdFind_00245337": REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "fragments"
    / "Quests"
    / "QF_SQ_WorkshopVertibirdFind_00245337.psc",
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_source(script_name: str) -> str:
    source_path = SCRIPT_CASES[script_name]
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_grenade_patch_sends_the_bound_story_event_and_handles_failure():
    patch = _script_patch_source("WorkshopVertibirdGrenadeScript")

    assert patch is not None
    assert _member_names(patch) == ["oninit"]
    assert "WorkshopVertibirdGrenadeKW.SendStoryEventAndWait(" in patch
    assert "GetCurrentLocation(), Self" in patch
    assert "SQ_WorkshopVertibirdFailMessage.Show()" in patch
    assert "AddKeyword" not in patch


def test_quest_fragment_patch_covers_arrival_return_and_death():
    patch = _script_patch_source(
        "Fragments:Quests:QF_SQ_WorkshopVertibirdFind_00245337"
    )

    assert patch is not None
    assert set(_member_names(patch)) == {
        "fragment_stage_0010_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "actor.ondeath",
    }
    assert "vertibirdRef.Enable()" in patch
    assert patch.count("vertibirdRef.EvaluatePackage()") == 2
    assert "SQ_WorkshopVertibirdSuccessMessage.Show()" in patch
    assert "SQ_WorkshopVertibirdReturnMessage.Show()" in patch
    assert "SQ_WorkshopVertibirdDeathMessage.Show()" in patch


@pytest.mark.parametrize("script_name", SCRIPT_CASES)
def test_production_merge_is_unique_and_idempotent(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_source(script_name)

    for member_name in _member_names(patch):
        assert _member_names(merged).count(member_name) == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", SCRIPT_CASES)
def test_full_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    source_path = SCRIPT_CASES[script_name]
    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(source_path.relative_to(source_path.parents[3])),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
