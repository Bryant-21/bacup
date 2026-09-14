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
TUTORIAL_SCRIPT = "Fragments:Quests:QF_Tutorial_PlaceCAMP_0046F08A"
TENTATIVE_PLANS_SCRIPT = "Fragments:Quests:QF_RSVP03_Quest_003A1FE0"
PATCH_MEMBERS = {
    TUTORIAL_SCRIPT: {
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
    },
    TENTATIVE_PLANS_SCRIPT: {
        "fragment_stage_2000_item_00",
        "fragment_stage_2100_item_00",
        "fragment_stage_2200_item_00",
        "fragment_stage_2300_item_00",
        "fragment_stage_2400_item_00",
        "fragment_stage_9000_item_00",
    },
}
EXACT_MEMBER_ONLY_SCRIPTS = {TUTORIAL_SCRIPT}


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize("script_name", PATCH_MEMBERS)
def test_camp_quest_patch_is_member_only(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    patch_members = set(_member_names(patch))
    if script_name in EXACT_MEMBER_ONLY_SCRIPTS:
        assert patch_members == PATCH_MEMBERS[script_name]
    else:
        assert PATCH_MEMBERS[script_name] <= patch_members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


def test_tutorial_displays_then_completes_place_camp_objective():
    patch = _script_patch_source(TUTORIAL_SCRIPT)
    assert patch is not None
    start = _member_body(patch, "fragment_stage_0010_item_00")
    complete = _member_body(patch, "fragment_stage_0020_item_00")
    assert "SetValue(Tutorial_PlaceCAMPStarted, 1.0)" in start
    assert "SetObjectiveDisplayed(10)" in start
    assert "SetObjectiveCompleted(10)" in complete


def test_tentative_plans_selects_new_or_existing_camp_branch():
    patch = _script_patch_source(TENTATIVE_PLANS_SCRIPT)
    assert patch is not None
    deployed = _member_body(patch, "fragment_stage_2100_item_00")
    assert "GetValue(pRSVP03_AV_StashObjective) > 0.0" in deployed
    assert deployed.count("SetObjectiveDisplayed(2200)") == 1
    assert deployed.count("SetObjectiveDisplayed(2300)") == 1
    assert deployed.count("SetObjectiveDisplayed(2400)") == 1
    assert "Scene_BuildGenerator.Start()" in deployed


def test_tentative_plans_build_objectives_converge_on_stage_2500():
    patch = _script_patch_source(TENTATIVE_PLANS_SCRIPT)
    assert patch is not None
    cooking = _member_body(patch, "fragment_stage_2200_item_00")
    stash = _member_body(patch, "fragment_stage_2300_item_00")
    generator = _member_body(patch, "fragment_stage_2400_item_00")
    assert "IsStageDone(2300)" in cooking
    assert "IsStageDone(2200)" in stash
    assert "SetStage(2500)" in cooking
    assert "SetStage(2500)" in stash
    assert "SetStage(2500)" in generator


def test_tentative_plans_success_records_completion_and_starts_scene():
    patch = _script_patch_source(TENTATIVE_PLANS_SCRIPT)
    assert patch is not None
    success = _member_body(patch, "fragment_stage_9000_item_00")
    assert "SetValue(pRSVP03_AV_QuestCompletion, 1.0)" in success
    assert "Scene_QuestComplete.Start()" in success


@pytest.mark.parametrize("script_name", PATCH_MEMBERS)
def test_camp_quest_production_merge_is_unique_and_idempotent(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member_name in PATCH_MEMBERS[script_name]:
        assert names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", PATCH_MEMBERS)
def test_camp_quest_full_production_merge_native_compiles_for_fo4(
    script_name: str,
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
