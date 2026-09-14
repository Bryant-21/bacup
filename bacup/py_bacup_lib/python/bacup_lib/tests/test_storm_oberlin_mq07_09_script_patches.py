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
MQ07_FRAGMENT = "Fragments:Quests:QF_Storm_MQ07_OberlinPt1_0072C01B"
MQ08_FRAGMENT = "Fragments:Quests:QF_Storm_MQ08_OberlinPt2_006FBF34"
MQ09_FRAGMENT = "Fragments:Quests:QF_Storm_MQ09_OberlinPt3_006FBF37"
MQ07_PIPES = "Quests:Storm:MQ07:RepairablePipesAliasScript"
MQ08_SEDATION = "Quests:Storm:MQ08:LostSedationAliasScript"
MQ08_VAT = "Quests:Storm:MQ08:VatStorageCutsceneDummyAliasScript"
PATCHED_SCRIPTS = (
    MQ07_FRAGMENT,
    MQ08_FRAGMENT,
    MQ09_FRAGMENT,
    MQ07_PIPES,
    MQ08_SEDATION,
    MQ08_VAT,
)

DECLARED_FRAGMENT_STAGES = {
    MQ07_FRAGMENT: {
        100, 200, 300, 400, 410, 415, 417, 420, 425, 427, 430, 480, 499,
        500, 510, 550, 600, 610, 700, 9000, 9999,
    },
    MQ08_FRAGMENT: {
        100, 200, 300, 350, 400, 410, 420, 500, 510, 520, 600, 700, 800,
        850, 900, 1000, 1100, 1200, 1300, 1310, 1320, 1400, 1410, 1420,
        1430, 1500, 1600, 1700, 1710, 1715, 1720, 1800, 1900, 2000,
        2100, 2101, 2105, 2106, 2200, 2300, 2400, 2401, 2402, 2405, 2410,
        2420, 2500, 9000, 9999,
    },
    MQ09_FRAGMENT: {
        100, 200, 300, 310, 311, 315, 320, 330, 340, 350, 360, 600, 605,
        610, 620, 630, 700, 9000, 9999,
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


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_oberlin_patch_is_member_only(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "endstate"))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_oberlin_production_merge_is_unique_and_idempotent(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    merged_names = _member_names(merged)

    for member_name in _member_names(patch):
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", DECLARED_FRAGMENT_STAGES)
def test_oberlin_fragments_only_implement_declared_stages(script_name: str):
    implemented = {
        int(name[len("fragment_stage_") : len("fragment_stage_") + 4])
        for name in _member_names(_script_patch_source(script_name))
        if name.startswith("fragment_stage_")
    }
    assert implemented <= DECLARED_FRAGMENT_STAGES[script_name]


def test_mq07_lower_atrium_gate_ignores_redundant_417_but_requires_other_tasks():
    patch = _script_patch_source(MQ07_FRAGMENT)
    body = _member_body(patch, "loweratriumtaskscomplete")
    return_line = next(
        line.strip() for line in body.splitlines()
        if line.strip().lower().startswith("return ")
    )
    assert return_line == (
        "Return IsStageDone(410) && IsStageDone(420) && IsStageDone(430)"
    )

    advance_body = _member_body(patch, "advancewhenloweratriumtaskscomplete")
    assert "LowerAtriumTasksComplete()" in advance_body
    assert "SetStage(499)" in advance_body


def test_mq08_reconciles_bound_items_scenes_and_choices():
    patch = _script_patch_source(MQ08_FRAGMENT)
    assert "Storm_MQ08_OrganicsSecurityKey" in _member_body(
        patch, "fragment_stage_0200_item_00"
    )
    for stage, scene_property in (
        (700, "Storm_MQ08_OberlinPt2_HildaIntercom01_Greenhouse"),
        (1200, "Storm_MQ08_OberlinPt2_HildaIntercom02_DrugsLab"),
        (2000, "Storm_MQ08_OberlinPt2_HildaIntercom03_Surgery"),
        (2400, "Storm_MQ08_OberlinPt2_HildaIntercom04_VatStorage"),
    ):
        assert scene_property in _member_body(
            patch, f"fragment_stage_{stage:04d}_item_00"
        )
    assert "Storm_MQ08_HildaChoice, 1.0" in _member_body(
        patch, "fragment_stage_2410_item_00"
    )
    assert "Storm_MQ08_HildaChoice, 2.0" in _member_body(
        patch, "fragment_stage_2420_item_00"
    )


def test_mq09_preserves_bribe_choice_and_story_handoff():
    patch = _script_patch_source(MQ09_FRAGMENT)
    assert "AddItem(Caps001, 50" in _member_body(
        patch, "fragment_stage_0605_item_00"
    )
    assert "Storm_MQ09_OberlinChoice, 1.0" in _member_body(
        patch, "fragment_stage_0620_item_00"
    )
    assert "Storm_MQ09_OberlinChoice, 2.0" in _member_body(
        patch, "fragment_stage_0630_item_00"
    )
    completion = _member_body(patch, "fragment_stage_9000_item_00")
    tracker_read = (
        "Float trackerValue = player.GetValue(Storm_MQ00_QuestProgressionTracker)"
    )
    tracker_write = "player.SetValue(Storm_MQ00_QuestProgressionTracker, trackerValue)"
    story_event = "Storm_MQ13_Finale_StartKeyword.SendStoryEvent()"
    assert completion.count(tracker_read) == 1
    assert completion.count("If trackerValue < 2.0") == 1
    assert completion.count("trackerValue += 1.0") == 1
    assert completion.count("If trackerValue > 2.0") == 1
    assert completion.count("trackerValue = 2.0") == 1
    assert completion.count(tracker_write) == 1
    assert completion.count("If trackerValue >= 2.0") == 1
    assert completion.count("startFinale = True") == 1
    assert completion.count("If startFinale") == 1
    assert completion.count(story_event) == 1
    assert completion.index(tracker_read) < completion.index("If trackerValue < 2.0")
    assert completion.index("trackerValue += 1.0") < completion.index(tracker_write)
    assert completion.index(tracker_write) < completion.index("If trackerValue >= 2.0")
    assert completion.index("If trackerValue >= 2.0") < completion.index(
        "startFinale = True"
    )
    assert completion.index(
        "SetValue(Storm_MQ_HildaAwayValue, 0.0)"
    ) < completion.index("If startFinale")
    assert completion.index("If startFinale") < completion.index(story_event)
    assert "ModValue(Storm_MQ00_QuestProgressionTracker" not in completion


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_oberlin_full_production_merge_native_compiles_for_fo4(script_name: str):
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


def test_mq07_generator_door_unlock_displays_the_never_shown_critter_objective():
    patch = _script_patch_source("Fragments:Quests:QF_Storm_MQ07_OberlinPt1_0072C01B")
    body = _member_body(patch, "fragment_stage_0415_item_00")
    assert "SetObjectiveDisplayed(35)" in body
    other_displays = [
        line
        for line in patch.splitlines()
        if "SetObjectiveDisplayed(35)" in line
    ]
    assert len(other_displays) == 1


@pytest.mark.parametrize(
    "member,objectives",
    (
        ("fragment_stage_0510_item_00", (50, 55)),
        ("fragment_stage_0610_item_00", (60, 61)),
    ),
)
def test_mq07_return_checkpoints_reassert_only_incomplete_objectives(
    member: str, objectives: tuple[int, int]
):
    body = _member_body(
        _script_patch_source("Fragments:Quests:QF_Storm_MQ07_OberlinPt1_0072C01B"),
        member,
    )
    for index in objectives:
        assert f"!IsObjectiveCompleted({index})" in body
        assert f"SetObjectiveDisplayed({index})" in body
    assert "SetObjectiveCompleted(" not in body
    assert "SetStage(" not in body
