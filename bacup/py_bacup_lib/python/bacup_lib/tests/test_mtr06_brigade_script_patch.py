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
SCRIPT_NAME = "Fragments:Quests:QF_MTR06_Brigade_0003363B"
QUEST_SCRIPT_NAME = "MTR06_QuestScript"

SOURCE_FRAGMENT_STAGES = (
    3,
    4,
    5,
    10,
    15,
    20,
    29,
    30,
    31,
    40,
    45,
    46,
    50,
    55,
    60,
    65,
    70,
    80,
    90,
    100,
    999,
)
HELPER_MEMBERS = {
    "mtr06_getplayer",
    "mtr06_recordstage",
    "mtr06_givealiasitem",
}
PATCH_MEMBERS = {
    f"fragment_stage_{stage:04d}_item_00" for stage in SOURCE_FRAGMENT_STAGES
} | HELPER_MEMBERS


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
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _patch_source(script_name: str = SCRIPT_NAME) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _merged_source(script_name: str = SCRIPT_NAME) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch_source(script_name)
    )


def _checkpoint_target(checkpoint_value: float, current_stage: int) -> int | None:
    milestones = ((4, 70), (3, 31), (2, 20), (1, 10))
    target = next(
        (stage for value, stage in milestones if checkpoint_value >= value), None
    )
    if target is None or current_stage >= target:
        return None
    return target


def test_mtr06_patch_matches_all_source_declared_fragment_members() -> None:
    patch = _patch_source()
    member_names = _member_names(patch)

    assert len(member_names) == len(PATCH_MEMBERS)
    assert set(member_names) == PATCH_MEMBERS
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state ", "extends "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


def test_mtr06_production_merge_is_unique_and_idempotent() -> None:
    patch = _patch_source()
    merged = _merged_source()
    merged_members = _member_names(merged)

    for member in PATCH_MEMBERS:
        assert merged_members.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged


def test_mtr06_production_merge_compiles(tmp_path: Path) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_mtr06_checkpoint_controller_merges_once_and_compiles() -> None:
    patch = _patch_source(QUEST_SCRIPT_NAME)
    merged = _merged_source(QUEST_SCRIPT_NAME)
    patch_members = _member_names(patch)
    merged_members = _member_names(merged)

    for member in patch_members:
        assert merged_members.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(QUEST_SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_mtr06_checkpoint_controller_selects_source_milestones_without_regression() -> None:
    patch = _patch_source(QUEST_SCRIPT_NAME)
    reconcile = _member_body(patch, "mtr06_reconcilecheckpoint")

    assert "fStartupCheckpointVal >= iCompletedFinalAV" in reconcile
    assert "targetStage = iFinalWrappedUpStage" in reconcile
    assert "fStartupCheckpointVal >= iCompletedPEAV" in reconcile
    assert "targetStage = iPhysicalWrapUpStage" in reconcile
    assert "fStartupCheckpointVal >= iCompletedKnowledgeAV" in reconcile
    assert "targetStage = iKnowledgeExamStage" in reconcile
    assert "fStartupCheckpointVal >= iCompletedMiscAV" in reconcile
    assert "targetStage = iMiscCompletedStage" in reconcile
    assert "GetStage() < targetStage" in reconcile
    assert "!IsStageDone(targetStage)" in reconcile
    assert reconcile.index("bStartupOnce = True") < reconcile.index("SetStage(targetStage)")

    physical = _member_body(patch, "mtr06_handlephysicalexamcomplete")
    assert "playerRef.GetValue(MTR06_PhysExamCompleted) >= 1.0" in physical
    assert "GetStage() < iPhysicalCompleteStage" in physical
    assert "!IsStageDone(iPhysicalCompleteStage)" in physical
    assert "SetStage(iPhysicalCompleteStage)" in physical


@pytest.mark.parametrize(
    ("checkpoint_value", "current_stage", "expected"),
    [
        (0.0, 4, None),
        (1.0, 4, 10),
        (2.0, 4, 20),
        (3.0, 4, 31),
        (4.0, 4, 70),
        (4.0, 80, None),
        (2.0, 20, None),
    ],
)
def test_mtr06_checkpoint_transition_matrix(
    checkpoint_value: float, current_stage: int, expected: int | None
) -> None:
    assert _checkpoint_target(checkpoint_value, current_stage) == expected


def test_mtr06_exam_progression_uses_source_stage_checkpoints() -> None:
    patch = _patch_source()

    knowledge = _member_body(patch, "fragment_stage_0020_item_00")
    assert "SetObjectiveCompleted(10)" in knowledge
    assert "SetObjectiveDisplayed(20)" in knowledge
    assert "Alias_PlayerReadyForPhysExam.ForceRefTo(playerRef)" in knowledge
    assert "playerRef.SetValue(MTR06_CheckpointValue, 2.0)" in knowledge
    assert "SetStage(22)" in knowledge

    for source_stage in (29, 30):
        physical = _member_body(
            patch, f"fragment_stage_{source_stage:04d}_item_00"
        )
        assert "playerRef.SetValue(MTR06_PhysExamCompleted, 1.0)" in physical
        assert "SetStage(31)" in physical
        assert physical.index(f"MTR06_RecordStage({source_stage})") < physical.index(
            "SetStage(31)"
        )

    wrapup = _member_body(patch, "fragment_stage_0031_item_00")
    assert "SetObjectiveCompleted(20)" in wrapup
    assert "SetObjectiveDisplayed(30)" in wrapup
    assert "Alias_PlayerReadyForFinalExam.ForceRefTo(playerRef)" in wrapup
    assert "playerRef.SetValue(MTR06_CheckpointValue, 3.0)" in wrapup
    assert "SetStage(32)" in wrapup


def test_mtr06_final_exam_kit_and_mine_are_reentry_safe() -> None:
    patch = _patch_source()

    kit = _member_body(patch, "fragment_stage_0050_item_00")
    assert "playerRef.GetItemCount(ClothesFiremanUniform) < 1" in kit
    assert "playerRef.GetItemCount(ClothesFiremanHat) < 1" in kit
    assert "MTR06_GiveAliasItem(playerRef, Alias_AntiScorched10mm)" in kit
    assert "MTR06_GiveAliasItem(playerRef, Alias_FinalExamHolotape)" in kit
    assert "playerRef.SetValue(MTR06_GaveOutMidQuestReward, 1.0)" in kit

    briefing = _member_body(patch, "fragment_stage_0055_item_00")
    assert "Alias_PlayerCanTriggerBeacon.ForceRefTo(playerRef)" in briefing
    assert "!MTR06_TrainingMine.IsRunning()" in briefing
    assert "MTR06_TrainingMine.Start()" in briefing

    return_stage = _member_body(patch, "fragment_stage_0070_item_00")
    assert "SetObjectiveCompleted(60)" in return_stage
    assert "SetObjectiveDisplayed(70)" in return_stage
    assert "MTR06_TrainingMine.Stop()" in return_stage
    assert "Alias_PlayerCanRegister.ForceRefTo(playerRef)" in return_stage
    assert "playerRef.SetValue(MTR06_CheckpointValue, 4.0)" in return_stage


def test_mtr06_dispatch_scene_completes_before_story_handoff() -> None:
    patch = _patch_source()

    dispatch = _member_body(patch, "fragment_stage_0090_item_00")
    assert "MTR06_Brigade_0090_MadiganRescue.Start()" in dispatch
    assert "SetStage(100)" not in dispatch

    completion = _member_body(patch, "fragment_stage_0100_item_00")
    assert "CompleteAllObjectives()" in completion
    assert "playerRef.SetValue(MTR06_QuestCompleted, 1.0)" in completion
    assert "MTR06_PostMisc_QuestStartKeyword.SendStoryEvent(akRef1 = playerRef)" in completion
    assert (
        "MTN_MQ_Missing_Quest_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)"
        in completion
    )
    assert "If nextQuestStarted && playerRef != None" in completion
    assert "playerRef.SetValue(MTN_MQ_StartedValue, 1.0)" in completion
    assert "MTN_MQ_Missing.Start()" not in completion


def test_mtr06_shutdown_clears_capability_aliases_and_subquests() -> None:
    shutdown = _member_body(_patch_source(), "fragment_stage_0999_item_00")

    for alias_name in (
        "Alias_PlayerReadyForKnowledgeExam",
        "Alias_PlayerReadyForPhysExam",
        "Alias_PlayerReadyForFinalExam",
        "Alias_PlayerCanCollectUniform",
        "Alias_PlayerCanTriggerBeacon",
        "Alias_PlayerCanRegister",
        "Alias_PlayerCanUseDispatch",
        "Alias_PlayerRegistered",
        "Alias_FinalExamRespawnTarget",
    ):
        assert f"{alias_name}.Clear()" in shutdown
    assert "MTR06_TrainingMine.Stop()" in shutdown
    assert "MTR06_Brigade_0035_FinalExamIntro.Stop()" in shutdown
    assert "MTR06_Brigade_0090_MadiganRescue.Stop()" in shutdown
