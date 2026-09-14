from __future__ import annotations

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

PATCH_MEMBERS = {
    "SFM01_Glow_ChemConsoleAliasScript": {"onactivate"},
    "SFM01_Glow_CatalystConsoleAliasScript": {"onactivate"},
}


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_MEMBERS.items())
def test_sfm04_console_patches_merge_once_and_compile(
    script_name: str, expected_members: set[str]
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname" not in patch
    assert "Property" not in patch

    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected_members:
        assert members.count(member) == 1
    assert set(members) == expected_members
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_sfm04_chemical_console_uses_only_the_selected_bound_row():
    patch = _script_patch_source("SFM01_Glow_ChemConsoleAliasScript")

    assert patch is not None
    assert "akActionRef != playerRef" in patch
    assert "selectedChem.ButtonPressed == MyButton" in patch
    assert "!owningQuest.IsStageDone(selectedChem.StageToSet)" in patch
    assert "playerRef.GetItemCount(selectedChem.ItemToRemove) > 0" in patch
    assert "playerRef.RemoveItem(selectedChem.ItemToRemove, 1, True)" in patch
    assert "ConsoleReference.PlayAnimation(selectedChem.AnimationToPlay)" in patch
    assert "owningQuest.SetStage(selectedChem.StageToSet)" in patch


def test_sfm04_catalyst_console_is_inventory_backed_and_idempotent():
    patch = _script_patch_source("SFM01_Glow_CatalystConsoleAliasScript")

    assert patch is not None
    assert "akActionRef != playerRef" in patch
    assert "selectedButton == 1" in patch
    assert "!owningQuest.IsStageDone(StageToSet)" in patch
    assert "playerRef.GetItemCount(CatalystToUse) > 0" in patch
    assert "playerRef.RemoveItem(CatalystToUse, 1, True)" in patch
    assert 'ConsoleReference.PlayAnimation("CatalystIn01")' in patch
    assert "owningQuest.SetStage(StageToSet)" in patch


def test_sfm04_stage_700_uses_the_explicit_fo4_no_crafting_substitute():
    patch = _script_patch_source("Fragments:Quests:QF_SFM04_Organic_0010AE02")

    assert patch is not None
    stage_start = patch.index("Function Fragment_Stage_0700_Item_00()")
    stage_end = patch.index("EndFunction", stage_start)
    stage = patch[stage_start:stage_end]
    assert "SetObjectiveCompleted(700, True)" in stage
    assert "playerRef.GetItemCount(SFM04_Organic_RadShield) <= 0" in stage
    assert "playerRef.AddItem(SFM04_Organic_RadShield, 1, False)" in stage
    assert "!IsStageDone(1000)" in stage
    assert "SetStage(1000)" in stage
    assert "ConstructibleObject" not in patch
    assert "Recipe" not in patch
