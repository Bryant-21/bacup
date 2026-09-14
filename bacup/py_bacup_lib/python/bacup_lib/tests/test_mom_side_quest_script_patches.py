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
    "Fragments:Quests:QF_MoM00_00345D50": {
        "fragment_stage_0020_item_00",
        "fragment_stage_0021_item_00",
        "fragment_stage_0031_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_MoM01_00345D52": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0057_item_00",
        "fragment_stage_0058_item_00",
        "fragment_stage_0059_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0062_item_00",
        "fragment_stage_0070_item_00",
        "fragment_stage_0080_item_00",
        "fragment_stage_0085_item_00",
        "fragment_stage_0090_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_MoM02_003472F4": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0021_item_00",
        "fragment_stage_0022_item_00",
        "fragment_stage_0023_item_00",
        "fragment_stage_0090_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_MoM03_00357E7E": {
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0055_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0070_item_00",
        "fragment_stage_0075_item_00",
        "fragment_stage_0080_item_00",
        "fragment_stage_0085_item_00",
        "fragment_stage_0090_item_00",
        "fragment_stage_0091_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_MoM04_00357E7A": {
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0041_item_00",
        "fragment_stage_0042_item_00",
        "fragment_stage_0043_item_00",
        "fragment_stage_0045_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0051_item_00",
        "fragment_stage_0052_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0090_item_00",
        "fragment_stage_0100_item_00",
    },
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_mom_patch_is_member_only_and_complete(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_mom_patch_merges_once_and_native_compiles(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member in members:
        assert names.count(member) == 1
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


def test_mom_promotion_chain_is_local_and_idempotent():
    mom02 = _script_patch_source("Fragments:Quests:QF_MoM02_003472F4")
    mom04 = _script_patch_source("Fragments:Quests:QF_MoM04_00357E7A")
    assert mom02 is not None
    assert mom04 is not None

    for stage in (21, 22, 23):
        body = _member_body(mom02, f"fragment_stage_00{stage}_item_00")
        assert "!IsStageDone(90)" in body
        assert "SetStage(90)" in body

    for stage, required_stage in ((51, 52), (52, 51)):
        body = _member_body(mom04, f"fragment_stage_00{stage}_item_00")
        assert f"IsStageDone({required_stage})" in body
        assert "!IsStageDone(60)" in body
        assert "SetStage(60)" in body


def test_mom_pickup_and_readme_handoffs_converge_without_root_script_logic():
    mom01 = _script_patch_source("Fragments:Quests:QF_MoM01_00345D52")
    mom03 = _script_patch_source("Fragments:Quests:QF_MoM03_00357E7E")
    assert mom01 is not None
    assert mom03 is not None

    for stage, required_stages in ((57, (58, 59)), (58, (57, 59)), (59, (57, 58))):
        body = _member_body(mom01, f"fragment_stage_00{stage}_item_00")
        for required_stage in required_stages:
            assert f"IsStageDone({required_stage})" in body
        assert "!IsStageDone(60)" in body
        assert "SetStage(60)" in body

    readme = _member_body(mom03, "fragment_stage_0091_item_00")
    assert "!IsStageDone(100)" in readme
    assert "SetStage(100)" in readme


def test_mom_rank_rewards_and_completion_are_bound_to_terminal_stages():
    expected_ranks = {
        "Fragments:Quests:QF_MoM01_00345D52": 2,
        "Fragments:Quests:QF_MoM02_003472F4": 3,
        "Fragments:Quests:QF_MoM04_00357E7A": 4,
    }

    for script_name, rank in expected_ranks.items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        completion = _member_body(patch, "fragment_stage_0100_item_00")
        assert f"player.SetValue(MoMRank, {rank}.0)" in completion
        assert "CompleteAllObjectives()" in completion
        assert "Stop()" in completion
