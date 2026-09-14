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
QUEST_FRAGMENT = "Fragments:Quests:QF_FS02_MQ_Reassembly_004E0719"
QUEST_SCRIPT = "FS02_MQ_Reassembly_QuestScript"
PLAYER_SCRIPT = "FS02_MQ_Reassembly_PlayerAliasScript"
DETECTOR_SCRIPT = "FS02_MQ_Reassembly_DetectorScript"
SCRIPTS = (QUEST_FRAGMENT, QUEST_SCRIPT, PLAYER_SCRIPT, DETECTOR_SCRIPT)
FRAGMENT_STAGES = (
    0,
    10,
    25,
    50,
    100,
    150,
    200,
    205,
    210,
    220,
    230,
    240,
    250,
    260,
    300,
    400,
    410,
    450,
    500,
    600,
    1000,
)
EXPECTED_MEMBERS = {
    QUEST_FRAGMENT: {
        f"fragment_stage_{stage:04d}_item_00" for stage in FRAGMENT_STAGES
    }
    | {"fs02_trystartfruition", "ontimer"},
    QUEST_SCRIPT: {
        "recountupgradeddetectors",
        "restorecheckpointinventory",
        "onquestinit",
        "onstageset",
    },
    PLAYER_SCRIPT: {
        "evaluateabbiesbunkerentry",
        "onaliasinit",
        "onplayerloadgame",
        "resettrackeditemfilters",
        "onlocationchange",
        "onitemadded",
        "onaliasshutdown",
    },
    DETECTOR_SCRIPT: {"onactivate"},
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


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_fs02_patches_are_member_only_and_merge_idempotently(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert set(_member_names(patch)) == EXPECTED_MEMBERS[script_name]
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )

    merged = _merged_source(script_name)
    names = _member_names(merged)
    for member in EXPECTED_MEMBERS[script_name]:
        assert names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_fs02_full_merged_sources_native_compile(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        _merged_source(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_fs02_fragment_patch_covers_all_source_bound_stages():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    assert {
        name for name in _member_names(patch) if name.startswith("fragment_stage_")
    } == {
        f"fragment_stage_{stage:04d}_item_00" for stage in FRAGMENT_STAGES
    }


def test_fs02_detector_consumes_one_transmitter_once_before_setting_bound_stage():
    patch = _script_patch_source(DETECTOR_SCRIPT)
    assert patch is not None
    body = _member_body(patch, "onactivate")

    assert "akActionRef != playerRef" in body
    assert "owningQuest.GetStageDone(StageToSet)" in body
    assert (
        "playerRef.GetItemCount(FS01_MQ_Warn_UpgradedTransmitter) < 1" in body
    )
    assert "DetectorAlias.ForceRefTo(detectorRef)" in body
    remove = (
        "playerRef.RemoveItem(FS01_MQ_Warn_UpgradedTransmitter, 1, True)"
    )
    set_stage = "owningQuest.SetStage(StageToSet)"
    assert body.count(remove) == 1
    assert body.count(set_stage) == 1
    assert body.index(remove) < body.index(set_stage)


def test_fs02_detector_stages_update_checkpoints_and_join_at_260():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    stages = (210, 220, 230, 240, 250)
    for detector_index, stage in enumerate(stages, start=1):
        body = _member_body(
            patch, f"fragment_stage_{stage:04d}_item_00"
        )
        assert f"FS02_CheckpointDetector0{detector_index}" in body
        for peer in stages:
            if peer != stage:
                assert f"GetStageDone({peer})" in body
        assert "SetStage(260)" in body

    finish = _member_body(patch, "fragment_stage_0260_item_00")
    assert "SetObjectiveCompleted(200, True)" in finish
    assert "SetValue(FS02_CheckpointValue, 300.0)" in finish
    assert "SetStage(300)" in finish


def test_fs02_controller_restores_remaining_checkpoint_inventory():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None
    restore = _member_body(patch, "restorecheckpointinventory")
    recount = _member_body(patch, "recountupgradeddetectors")

    assert "Int transmittersNeeded = 5 - DetectorCount" in restore
    assert "CPUpgradedTransmitters.GetAt(checkpointIndex)" in restore
    assert "checkpointItem.GetContainer().RemoveItem" in restore
    assert "CPUplink.GetReference()" in restore
    assert "checkpointUplink.GetContainer().RemoveItem" in restore
    for stage in (210, 220, 230, 240, 250):
        assert f"GetStageDone({stage})" in recount


def test_fs02_only_completes_fs01_after_successful_successor_initialization():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None
    initialized = _member_body(patch, "onquestinit")

    assert "Parent.OnQuestInit()" in initialized
    assert "!FS01_MQ_Warn.GetStageDone(1000)" in initialized
    assert "FS01_MQ_Warn.SetStage(1000)" in initialized
    assert initialized.index("Parent.OnQuestInit()") < initialized.index(
        "FS01_MQ_Warn.SetStage(1000)"
    )


def test_fs02_objectives_scenes_handoff_and_completion_form_one_chain():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    abbie = _member_body(patch, "fragment_stage_0050_item_00")
    rose = _member_body(patch, "fragment_stage_0400_item_00")
    uplink = _member_body(patch, "fragment_stage_0410_item_00")
    handoff = _member_body(patch, "fragment_stage_0600_item_00")
    start_helper = _member_body(patch, "fs02_trystartfruition")
    retry = _member_body(patch, "ontimer")
    completion = _member_body(patch, "fragment_stage_1000_item_00")

    assert "FS02_MQ_Reassembly_AbbieIntroScene.Start()" in abbie
    assert "FS02_MQ_Overdue_RoseScene.Start()" in rose
    assert "RemoveItem(FS01_MQ_Warn_UplinkMiscItem, 1, True)" in uplink
    assert "FS02_TryStartFruition()" in handoff
    assert "SetStage(1000)" in handoff
    assert "StartTimer(5.0, 600)" in handoff
    assert (
        'Game.GetFormFromFile(0x00012566, "SeventySix.esm") as Quest'
        in start_helper
    )
    assert "fruitionQuest.IsRunning() || fruitionQuest.IsCompleted()" in start_helper
    assert (
        "FS03_MQ_Fruition_QuestStartKeyword.SendStoryEventAndWait"
        in start_helper
    )
    assert "Return accepted ||" in start_helper
    assert "aiTimerID != 600" in retry
    assert "GetStageDone(1000)" in retry
    assert "FS02_TryStartFruition()" in retry
    assert "StartTimer(5.0, 600)" in retry
    assert "CompleteQuest()" in completion
    assert "Stop()" in completion


def test_fs02_player_alias_starts_bunker_step_and_tracks_real_inventory_refs():
    patch = _script_patch_source(PLAYER_SCRIPT)
    assert patch is not None
    evaluate = _member_body(patch, "evaluateabbiesbunkerentry")
    added = _member_body(patch, "onitemadded")

    assert "playerRef.GetCurrentLocation() == AbbiesBunkerLocation" in evaluate
    assert "owningQuest.SetStage(50)" in evaluate
    assert "Uplink.ForceRefTo(akItemReference)" in added
    assert "UpgradedTransmitters.AddRef(akItemReference)" in added
