from __future__ import annotations

import re
from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)


_DECLARATION_LINE = re.compile(
    r"^\s*(?:Scriptname|Struct|(?:Auto\s+)?State|[A-Za-z_][\w:]*(?:\[\])?\s+(?:Property\s+)?[A-Za-z_]\w*\s*(?:=|Auto))",
    re.IGNORECASE,
)

REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

# Stage numbers declared by each course QUST's VMAD fragment table. Fragments for
# undeclared stages are pruned silently, so these sets are the contract.
FRAGMENT_STAGES = {
    "Fragments:Quests:QF_EN05_MarksmanshipTraining_0008D23B": {
        10,
        20,
        100,
        105,
        110,
        120,
        150,
    },
    "Fragments:Quests:QF_EN05_ObstacleCourse_0009C824": {
        10,
        20,
        30,
        40,
        99,
        100,
        105,
        110,
        120,
        150,
    },
    "Fragments:Quests:QF_EN05_CombatCourse_00052A62": {
        10,
        20,
        30,
        32,
        40,
        42,
        100,
        110,
        120,
        150,
    },
    "Fragments:Quests:QF_EN05_PatriotismCourse_0008C881": {
        10,
        20,
        30,
        40,
        41,
        50,
        60,
        70,
        80,
        81,
        90,
        100,
        105,
        110,
        120,
        150,
        151,
    },
}

# Declared stages the patch deliberately leaves hollow, with the reason.
# 151 is EN05_PatriotismCourse's RunOnStop shutdown stage; no record evidence
# names anything it must clean up, so it stays unimplemented rather than guessed.
UNIMPLEMENTED_STAGES = {
    "Fragments:Quests:QF_EN05_PatriotismCourse_0008C881": {151},
}

# Course quest script -> the fragment script that drives it.
COURSE_SCRIPTS = {
    "EN05_MarksmanshipCourseScript": (
        "Fragments:Quests:QF_EN05_MarksmanshipTraining_0008D23B"
    ),
    "EN05_ObstacleCourseQuestScript": (
        "Fragments:Quests:QF_EN05_ObstacleCourse_0009C824"
    ),
    "EN05_PatriotismTrainingQuestScript": (
        "Fragments:Quests:QF_EN05_PatriotismCourse_0008C881"
    ),
    # Combat inherits the reporting helper from EN05_CourseScript.
    "EN05_CourseScript": "Fragments:Quests:QF_EN05_CombatCourse_00052A62",
}

ALL_PATCHED_SCRIPTS = (
    "EN05_QuestScript",
    "EN05_CourseScript",
    "EN05_CombatCourseScript",
    "EN05_MarksmanshipCourseScript",
    "EN05_ObstacleCourseQuestScript",
    "EN05_PatriotismTrainingQuestScript",
    "EN05_TargetAliasCollectionScript",
    "EN05_ObstacleAliasCollectionScript",
    *FRAGMENT_STAGES,
)


def _members(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _stage_members(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind == "function" and name.startswith("fragment_stage_")
    ]


def _member_source(source: str, member_name: str) -> str:
    lines = source.splitlines()
    for _kind, name, start, end in _iter_top_level_papyrus_members(lines):
        if name == member_name.lower():
            return "\n".join(lines[start : end + 1])
    raise AssertionError(f"missing Papyrus member: {member_name}")


def _skeleton(script_name: str) -> str:
    return (SOURCE_ROOT / _script_relative_path(script_name, ".psc")).read_text(
        encoding="utf-8"
    )


@pytest.mark.parametrize("script_name", ALL_PATCHED_SCRIPTS)
def test_en05_course_patches_merge_once_and_are_idempotent(script_name: str) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None, f"no patch for {script_name}"
    assert "Scriptname" not in patch
    # The merger keeps declarations from the skeleton; a patch must add members only.
    declarations = [
        line
        for line in patch.splitlines()
        if line[:1].strip()
        and _DECLARATION_LINE.match(line)
        and "Function" not in line
        and "Event" not in line
    ]
    assert declarations == []
    assert all(count == 1 for count in Counter(_members(patch)).values())

    merged = _merge_script_method_patches(_skeleton(script_name), patch)
    assert all(count == 1 for count in Counter(_members(merged)).values())
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("fragment_name,stages", sorted(FRAGMENT_STAGES.items()))
def test_en05_course_fragments_cover_exactly_the_declared_stages(
    fragment_name: str, stages: set[int]
) -> None:
    patch = _script_patch_source(fragment_name)
    assert patch is not None
    implemented = stages - UNIMPLEMENTED_STAGES.get(fragment_name, set())
    expected = {f"fragment_stage_{stage:04d}_item_00" for stage in implemented}
    merged = _merge_script_method_patches(_skeleton(fragment_name), patch)
    # Every implemented member maps to a declared stage (undeclared ones are
    # pruned silently at conversion time) and appears exactly once.
    assert Counter(_stage_members(patch)) == Counter(expected)
    assert Counter(_stage_members(merged)) == Counter(expected)


def test_en05_course_patches_keep_generated_declarations() -> None:
    merged = _merge_script_method_patches(
        _skeleton("EN05_ObstacleCourseQuestScript"),
        _script_patch_source("EN05_ObstacleCourseQuestScript"),
    )
    assert "Scriptname EN05_ObstacleCourseQuestScript Extends Quest conditional" in merged
    assert "TargetDatum[] Property TargetData Auto mandatory" in merged
    assert "Int Property iNextTargetCollection = 0 Auto conditional" in merged
    assert "Struct TargetDatum" in merged


@pytest.mark.parametrize("course_script,fragment_name", sorted(COURSE_SCRIPTS.items()))
def test_each_course_reports_completion_to_en05_basic(
    course_script: str, fragment_name: str
) -> None:
    course_patch = _script_patch_source(course_script)
    assert course_patch is not None
    report = _member_source(course_patch, "EN05Course_ReportCompletion")
    assert 'Game.GetFormFromFile(0x0008C87F, "SeventySix.esm") as EN05_QuestScript' in report
    assert "basic.EN05Basic_CourseCompleted(iCourseID)" in report

    fragment_patch = _script_patch_source(fragment_name)
    assert fragment_patch is not None
    success = _member_source(fragment_patch, "Fragment_Stage_0110_Item_00")
    assert "EN05Course_ReportCompletion()" in success
    assert "SetStage(150)" in success


def test_en05_basic_maps_course_ids_to_its_own_completion_stages() -> None:
    patch = _script_patch_source("EN05_QuestScript")
    assert patch is not None
    body = _member_source(patch, "EN05Basic_CourseCompleted")

    for course_id_var, stage_property in (
        ("iMarkmanshipCourseID", "iMarksmanshipCompleteStage"),
        ("iObstacleCourseID", "iObstacleCompleteStage"),
        ("iPatriotismCourseID", "iPatriotismCompleteStage"),
    ):
        assert f"aiCourseID == {course_id_var}" in body
        assert f"stageToSet = {stage_property}" in body

    # Combat splits on whether the three initial courses already finished.
    assert "IsStageDone(iInitialCoursesCompletedStage)" in body
    assert "stageToSet = iCombatCompleteStage" in body
    assert "stageToSet = iCombatCompletedEarlyStage" in body
    assert "!IsStageDone(stageToSet)" in body

    merged = _merge_script_method_patches(_skeleton("EN05_QuestScript"), patch)
    # The tally the EN05_Basic fragment runs reads exactly these stages.
    assert "Int Property iMarksmanshipCompleteStage = 30 Auto" in merged
    assert "Int Property iObstacleCompleteStage = 40 Auto" in merged
    assert "Int Property iPatriotismCompleteStage = 50 Auto" in merged
    assert "Int Property iCombatCompleteStage = 140 Auto" in merged
    assert "Int Property iCombatCompletedEarlyStage = 135 Auto" in merged


def test_marksmanship_targets_clear_to_the_bound_stage() -> None:
    patch = _script_patch_source("EN05_TargetAliasCollectionScript")
    assert patch is not None
    on_hit = _member_source(patch, "OnHit")
    # RegisterForHitEvent is single-shot, so every non-terminal path must re-arm.
    assert on_hit.count("RegisterForHitEvent(Self)") == 2
    assert "RemoveRef(akTarget)" in on_hit
    assert "owner.SetStage(iStageToSetOnClear)" in on_hit
    assert "!owner.IsStageDone(iStageToSetOnClear)" in on_hit

    merged = _merge_script_method_patches(
        _skeleton("EN05_TargetAliasCollectionScript"), patch
    )
    assert "Int Property iStageToSetOnClear = 100 Auto" in merged


def test_obstacle_sets_advance_only_when_the_active_collection_empties() -> None:
    collection_patch = _script_patch_source("EN05_ObstacleAliasCollectionScript")
    assert collection_patch is not None
    on_activate = _member_source(collection_patch, "OnActivate")
    assert "ActiveTargets.RemoveRef(akSenderRef)" in on_activate
    assert "If ActiveTargets.GetCount() > 0" in on_activate
    assert "course.EN05OB_AdvanceTargetSet()" in on_activate
    assert on_activate.index("ActiveTargets.RemoveRef") < on_activate.index(
        "EN05OB_AdvanceTargetSet"
    )

    quest_patch = _script_patch_source("EN05_ObstacleCourseQuestScript")
    assert quest_patch is not None
    advance = _member_source(quest_patch, "EN05OB_AdvanceTargetSet")
    assert "nextIndex >= TargetData.Length" in advance
    assert "SetStage(iCourseWrapupStage)" in advance
    assert "EN05OB_ActivateTargetSet(nextIndex)" in advance

    activate = _member_source(quest_patch, "EN05OB_ActivateTargetSet")
    assert "ActiveTargets.RemoveAll()" in activate
    assert "ActiveTargets.AddRefCollection(nextSet)" in activate
    assert "setScript.bObstaclesActive = True" in activate
    assert "TargetData[aiIndex].StartUpStage" in activate


def test_combat_waves_are_started_from_their_own_stages() -> None:
    patch = _script_patch_source("Fragments:Quests:QF_EN05_CombatCourse_00052A62")
    assert patch is not None
    for stage, wave in ((20, 0), (32, 1), (42, 2)):
        body = _member_source(patch, f"Fragment_Stage_{stage:04d}_Item_00")
        assert f"EN05CBTF_StartWave({wave})" in body

    starter = _member_source(patch, "EN05CBTF_StartWave")
    assert "as DefaultQuestEncounterWaveScript" in starter
    assert "waves.StartLocalEncounterWave(aiWaveIndex)" in starter

    cooldown = _script_patch_source("EN05_CombatCourseScript")
    assert cooldown is not None
    timer = _member_source(cooldown, "OnTimer")
    assert "SetStage(iWave02StartStage)" in timer
    assert "SetStage(iWave03StartStage)" in timer
    assert "Parent.OnTimer(aiTimerID)" in timer
