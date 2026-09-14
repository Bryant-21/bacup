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
QUEST_FRAGMENT = "Fragments:Quests:QF_FS03_MQ_Fruition_00012566"
QUEST_SCRIPT = "FS03_MQ_Fruition_QuestScript"
MASTER_HOLO_SCRIPT = "FS03_MQ_Fruition_MasterHoloScript"
SAM_TERMINAL_SCRIPT = "FS03_MQ_Fruition_TerminalScript"
MASTER_HOLO_TERMINAL_FRAGMENT = (
    "Fragments:Terminals:TERM_FS02_Fruition_NewHolota_0019C272"
)
SCRIPTS = (
    QUEST_FRAGMENT,
    QUEST_SCRIPT,
    MASTER_HOLO_SCRIPT,
    SAM_TERMINAL_SCRIPT,
    MASTER_HOLO_TERMINAL_FRAGMENT,
)
FRAGMENT_STAGES = (
    0,
    10,
    25,
    50,
    100,
    110,
    275,
    300,
    350,
    375,
    400,
    425,
    450,
    475,
    500,
    525,
    600,
    625,
    650,
    660,
    675,
    700,
    800,
    850,
    900,
    1000,
)
EXPECTED_MEMBERS = {
    QUEST_FRAGMENT: {
        f"fragment_stage_{stage:04d}_item_00" for stage in FRAGMENT_STAGES
    }
    | {"fs03_trystartbos01", "ontimer"},
    QUEST_SCRIPT: {"restoremasterholotape", "onquestinit", "onstageset"},
    MASTER_HOLO_SCRIPT: {"onholotapeplay"},
    SAM_TERMINAL_SCRIPT: {"onactivate"},
    MASTER_HOLO_TERMINAL_FRAGMENT: {"fragment_terminal_04"},
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
def test_fs03_patches_are_member_only_and_merge_idempotently(script_name: str):
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
def test_fs03_full_merged_sources_native_compile(script_name: str):
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


def test_fs03_fragment_patch_covers_all_source_bound_stages():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    assert {
        name for name in _member_names(patch) if name.startswith("fragment_stage_")
    } == {
        f"fragment_stage_{stage:04d}_item_00" for stage in FRAGMENT_STAGES
    }


def test_fs03_objectives_and_abbie_scenes_form_one_ordered_chain():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    intro = _member_body(patch, "fragment_stage_0050_item_00")
    armory = _member_body(patch, "fragment_stage_0350_item_00")
    alternate = _member_body(patch, "fragment_stage_0375_item_00")
    raleigh = _member_body(patch, "fragment_stage_0450_item_00")
    sam = _member_body(patch, "fragment_stage_0650_item_00")
    finish = _member_body(patch, "fragment_stage_0850_item_00")

    assert "FS03_MQ_Fruition_AbbieIntroScene.Start()" in intro
    assert "FS02_Fruition_AbbieArmoryEntranceScene.Start()" in armory
    assert "usedAlternateEntrance = !GetStageDone(350)" in alternate
    assert "FS02_Fruition_AbbieArmoryEntranceAltScene.Start()" in alternate
    assert "FS02_Fruition_AbbieArmoryTerminalScene.Start()" in raleigh
    assert "FS03_MQ_Fruition_AbbieSamTerminalScene.Start()" in sam
    assert "QSTFS03SystemReboot.Play(terminalRef)" in finish
    assert "FS02_Fruition_AbbieFinishScene.Start()" in finish

    transitions = {
        100: (50, 100),
        300: (275, 300),
        400: (375, 400),
        600: (475, 600),
        700: (675, 700),
        850: (800, 850),
    }
    for stage, (completed, displayed) in transitions.items():
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert f"SetObjectiveCompleted({completed}, True)" in body
        assert f"SetObjectiveDisplayed({displayed}, True, True)" in body


def test_fs03_master_holotape_advances_only_at_bound_terminals_and_stages():
    patch = _script_patch_source(MASTER_HOLO_SCRIPT)
    assert patch is not None
    play = _member_body(patch, "onholotapeplay")

    assert "akTerminalRef == FS03_MQ_Fruition_ArmoryTerminalRef" in play
    assert "currentStage == questController.iLoadHoloArmory" in play
    assert "SetStage(questController.iRunArmory)" in play
    assert "akTerminalRef == FS03_MQ_Fruition_RaleighTerminalRef" in play
    assert "SetStage(questController.iDownloadSchematics)" in play
    assert "akTerminalRef == FS03_MQ_Fruition_SamTerminalRef" in play
    assert "currentStage == 475 || currentStage == questController.iSamUnlocked" in play
    assert "!akTerminalRef.IsLocked()" in play
    assert "SetStage(questController.iDownloadCodes)" in play
    assert "akTerminalRef.HasKeyword(FS03_MQ_Fruition_RelayTerminalKeyword)" in play
    assert "SetStage(questController.iUploadData)" in play


def test_fs03_sam_and_master_terminal_controllers_gate_progression():
    sam_patch = _script_patch_source(SAM_TERMINAL_SCRIPT)
    terminal_patch = _script_patch_source(MASTER_HOLO_TERMINAL_FRAGMENT)
    assert sam_patch is not None
    assert terminal_patch is not None

    activate = _member_body(sam_patch, "onactivate")
    upload = _member_body(terminal_patch, "fragment_terminal_04")
    assert "akActionRef != playerRef" in activate
    assert "owningQuest.GetCurrentStageID() == 475" in activate
    assert "terminalRef.IsLocked()" in activate
    assert "owningQuest.SetStage(500)" in activate
    assert "FS02_Fruition.GetStageDone(700)" in upload
    assert "!FS02_Fruition.GetStageDone(800)" in upload
    assert "FS02_Fruition.SetStage(800)" in upload


def test_fs03_rewards_checkpoint_handoff_and_completion_are_repeat_safe():
    patch = _script_patch_source(QUEST_FRAGMENT)
    controller_patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None
    assert controller_patch is not None

    reward_stages = {475: 476, 675: 676, 800: 801}
    for source_stage, reward_stage in reward_stages.items():
        body = _member_body(
            patch, f"fragment_stage_{source_stage:04d}_item_00"
        )
        assert f"!GetStageDone({reward_stage})" in body
        assert f"SetStage({reward_stage})" in body

    access_codes = _member_body(patch, "fragment_stage_0625_item_00")
    upload_done = _member_body(patch, "fragment_stage_0800_item_00")
    assert "GetItemCount(FS02_Fruition_NirajPassword) == 0" in access_codes
    assert "AddItem(FS02_Fruition_NirajPassword, 1, True)" in access_codes
    assert "RemoveItem(FS02_Fruition_NirajPassword, 1, True)" in upload_done

    handoff = _member_body(patch, "fragment_stage_0900_item_00")
    start_helper = _member_body(patch, "fs03_trystartbos01")
    retry = _member_body(patch, "ontimer")
    completion = _member_body(patch, "fragment_stage_1000_item_00")
    assert "FS03_TryStartBoS01()" in handoff
    assert "SetStage(1000)" in handoff
    assert "StartTimer(5.0, 900)" in handoff
    assert "BoS01.IsRunning() || BoS01.IsCompleted()" in start_helper
    assert "BoS01_QuestStartKeyword.SendStoryEventAndWait" in start_helper
    assert "Return accepted ||" in start_helper
    assert "aiTimerID != 900" in retry
    assert "GetStageDone(1000)" in retry
    assert "FS03_TryStartBoS01()" in retry
    assert "StartTimer(5.0, 900)" in retry
    assert "SetValue(FS02_Fruition_QuestCompletedValue, 1.0)" in completion
    assert "CompleteQuest()" in completion
    assert "Stop()" in completion

    initialized = _member_body(controller_patch, "onquestinit")
    restore = _member_body(controller_patch, "restoremasterholotape")
    assert "Parent.OnQuestInit()" in initialized
    assert "!FS02_MQ_Reassembly.GetStageDone(1000)" in initialized
    assert "FS02_MQ_Reassembly.SetStage(1000)" in initialized
    assert "GetCurrentStageID() < iLoadHoloArmory" in restore
    assert "holotapeRef.GetContainer() == playerRef" in restore
    assert "RemoveItem(holotapeRef, 1, True, playerRef)" in restore
