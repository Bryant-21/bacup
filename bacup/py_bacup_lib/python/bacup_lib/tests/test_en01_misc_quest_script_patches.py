from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

FRAGMENT_SCRIPT = "Fragments:Quests:QF_EN01_Misc_000649C5"

# Only the six stages the EN01_Misc QUST VMAD actually declares.
DECLARED_FRAGMENT_STAGES = (4, 5, 6, 10, 20, 100)

PATCH_CASES = {
    "EN01_MiscQuest": {
        "onquestinit",
        "onstageset",
        "quest.onstageset",
        "initializeplayer",
        "restorecheckpoint",
        "startbunkerquest",
        "reconcilebunkerprogress",
    },
    "EN01_BlackwellInterviewNote": {
        "onread",
        "revealbunkermarker",
        "advancemiscquest",
    },
    FRAGMENT_SCRIPT: {
        f"fragment_stage_{stage:04d}_item_00"
        for stage in DECLARED_FRAGMENT_STAGES
    }
    | {"recordcheckpoint"},
}


def _members(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "expected"), PATCH_CASES.items())
def test_en01_misc_patch_merges_each_member_exactly_once(
    script_name: str, expected: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert "Scriptname " not in patch
    assert "Extends " not in patch
    assert " Property " not in patch

    merged = _merged(script_name)
    members = _members(merged)
    for member in expected:
        assert members.count(member) == 1, member
    assert _merge_script_method_patches(merged, patch) == merged


def test_en01_misc_fragment_only_covers_declared_stages():
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    stage_members = {
        name for name in _members(patch) if name.startswith("fragment_stage_")
    }
    assert stage_members == {
        f"fragment_stage_{stage:04d}_item_00"
        for stage in DECLARED_FRAGMENT_STAGES
    }


def test_en01_misc_checkpoint_is_monotonic_and_announced_once():
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    # A replayed or out-of-order stage must not roll the checkpoint backwards
    # or re-show the "Checkpointed." notification.
    assert (
        "If playerRef.GetValue(EN01_Misc_CheckpointValue) >= afCheckpointValue"
        in patch
    )
    assert patch.count("CheckpointMessage.Show()") == 1
    assert "RecordCheckpoint(1.0)" in patch
    assert "RecordCheckpoint(2.0)" in patch


def test_en01_misc_hands_off_to_the_bunker_quest_via_story_event():
    patch = _script_patch_source("EN01_MiscQuest")
    assert patch is not None
    assert "EN01_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)" in patch
    assert "EN01_MQ_Bunker_Master.Start()" not in patch
    assert "EN01_MQ_Bunker.Start()" not in patch
    assert "RegisterForRemoteEvent(EN01_MQ_Bunker, \"OnStageSet\")" in patch
    assert "Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)" in patch
    assert "EN01_MQ_Bunker.SetStage(iEN01ProgressStage)" in patch


def test_en01_misc_run_on_start_selects_the_bound_story_path():
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    assert "startNote = Alias_NoteObjectStory.GetRef()" in patch
    assert "Alias_NoteObject.ForceRefTo(startNote)" in patch
    assert "SetStage(5)" in patch
    assert "ElseIf !IsStageDone(6)" in patch
    assert "SetStage(6)" in patch


def test_en01_misc_reconciles_bunker_access_pad_progress_idempotently():
    patch = _script_patch_source("EN01_MiscQuest")
    assert patch is not None
    assert "auiStageID == iEN01BunkerStartUpStage" in patch
    assert "EN01_MQ_Bunker.IsStageDone(iEN01BunkerStartUpStage)" in patch
    assert "!EN01_MQ_Bunker.IsStageDone(iEN01ProgressStage)" in patch
    assert patch.count("ReconcileBunkerProgress()") == 3


def test_en01_misc_checkpoint_restore_runs_once_and_replays_earned_stages():
    patch = _script_patch_source("EN01_MiscQuest")
    assert patch is not None
    assert "If bCheckpointRestored" in patch
    assert "bCheckpointRestored = True" in patch
    assert "If !IsStageDone(iAcquiredQuinnsNotes)" in patch
    assert "If restoredValue >= ReadQuinnsNoteAV && !IsStageDone(iReadQuinnsNotesStage)" in patch


def test_en01_blackwell_note_reveals_marker_and_sets_read_stage():
    patch = _script_patch_source("EN01_BlackwellInterviewNote")
    assert patch is not None
    assert "SamsBunkerMarker.AddToMap(True)" in patch
    assert 'Game.GetFormFromFile(0x000649C5, "SeventySix.esm")' in patch
    assert "miscQuest.SetStage(20)" in patch
    assert "!miscQuest.IsStageDone(20)" in patch
