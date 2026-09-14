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
    "Quests:_Default:SetRandomStages": {"onstageset"},
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


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_quest_gap_patch_is_member_only_and_merges_once(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )

    merged = _merged_source(script_name)
    merged_names = _member_names(merged)
    for member in members:
        assert merged_names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_set_random_stages_preserves_local_quest_contract():
    patch = _script_patch_source("Quests:_Default:SetRandomStages")
    assert patch is not None
    assert "auiStageID != TriggerStage" in patch
    assert "IsStageDone(TurnOffStage)" in patch
    assert "Utility.RandomInt(0, PossibleStages.Length - 1)" in patch
    assert "stagesChecked < PossibleStages.Length" in patch
    assert "stagesSet < NumToSet" in patch
    assert "!IsStageDone(candidateStage)" in patch
    assert "SetStage(candidateStage)" in patch


def test_set_random_stage_children_inherit_the_repaired_parent():
    for child_name in (
        "DefaultSetRandomStagesA",
        "DefaultSetRandomStagesB",
        "DefaultSetRandomStagesC",
    ):
        child_source = (SOURCE_ROOT / f"{child_name}.psc").read_text(encoding="utf-8")
        assert "Extends quests:_default:setrandomstages" in child_source
        assert _member_names(child_source) == []


def test_quest_gap_full_production_sources_native_compile(tmp_path: Path):
    merged_source_root = tmp_path / "Scripts" / "Source" / "User"
    merged_sources = {
        script_name: _merged_source(script_name) for script_name in PATCH_MEMBERS
    }
    for script_name, merged in merged_sources.items():
        source_path = merged_source_root / _script_relative_path(script_name, ".psc")
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(merged, encoding="utf-8")

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    for script_name, merged in merged_sources.items():
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
