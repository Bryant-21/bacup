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
MAIN_QUEST = "Fragments:Quests:QF_BoS03_000183CA"
TRANSPONDER_QUEST = "Fragments:Quests:QF_BoS03_Transponders_000183CB"
TRANSPONDER_PERK = "Fragments:Perks:PRKF_BoS03UpdateTransponderP_004EC0E3"
SCENE_ACTIVATOR = "BoS03SceneActivatorScript"

PATCH_MEMBERS = {
    MAIN_QUEST: {
        "fragment_stage_0001_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0210_item_00",
        "fragment_stage_0220_item_00",
        "fragment_stage_0230_item_00",
        "fragment_stage_0240_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_9000_item_00",
        "bos03_trystarten01",
        "ontimer",
    },
    TRANSPONDER_QUEST: {
        "fragment_stage_0001_item_00",
        "fragment_stage_0002_item_00",
        "fragment_stage_0100_item_00",
    },
    TRANSPONDER_PERK: {"setbos03stage"},
    SCENE_ACTIVATOR: {"onactivate"},
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
def test_bos03_patches_are_exact_member_fragments(
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
        " property " in f" {line.strip().lower()} "
        for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_bos03_patches_merge_once_and_full_sources_compile(
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


def test_bos03_transponder_perk_preserves_selection_and_sets_the_quest_stage():
    merged = _merged_production_source(TRANSPONDER_PERK)
    entry = _member_body(merged, "fragment_entry_00")
    helper = _member_body(merged, "setbos03stage")

    for keyword, stage in [
        ("pLinkCustom01", 210),
        ("pLinkCustom02", 220),
        ("pLinkCustom03", 230),
        ("pLinkCustom04", 240),
        ("pLinkCustom05", 300),
    ]:
        assert f"akTargetRef.HasKeyword({keyword})" in entry
        assert f"nStage = {stage}" in entry
    assert "Self.SetBoS03Stage(akActor as Actor, nStage)" in entry
    assert "pQSTBoS03TransponderOn.Play" in helper
    assert "pBoS03 != None && nStage > 0" in helper
    assert "!pBoS03.IsStageDone(nStage)" in helper
    assert helper.count("pBoS03.SetStage(nStage)") == 1


def test_bos03_scene_and_transponder_quest_restore_local_replay_behavior():
    scene = _member_body(
        _script_patch_source(SCENE_ACTIVATOR) or "", "onactivate"
    )
    transponders = _script_patch_source(TRANSPONDER_QUEST)

    assert transponders is not None
    assert "akActionRef == Game.GetPlayer()" in scene
    assert "SceneToPlay != None && !SceneToPlay.IsPlaying()" in scene
    assert scene.count("SceneToPlay.Start()") == 1
    assert "transponder.Activate(playerRef)" in _member_body(
        transponders, "fragment_stage_0001_item_00"
    )
    assert "pBoS03_Transponders_01.Start()" in _member_body(
        transponders, "fragment_stage_0002_item_00"
    )
    assert "pBoS03_Transponders_01.Stop()" in _member_body(
        transponders, "fragment_stage_0100_item_00"
    )


def test_bos03_main_stages_restore_objectives_checkpoint_and_cleanup():
    quest = _script_patch_source(MAIN_QUEST)
    assert quest is not None

    start = _member_body(quest, "fragment_stage_0001_item_00")
    begin_search = _member_body(quest, "fragment_stage_0200_item_00")
    checkpoint = _member_body(quest, "fragment_stage_0240_item_00")
    final_transponder = _member_body(quest, "fragment_stage_0300_item_00")
    access = _member_body(quest, "fragment_stage_0500_item_00")
    end = _member_body(quest, "fragment_stage_0600_item_00")
    handoff_helper = _member_body(quest, "bos03_trystarten01")
    retry = _member_body(quest, "ontimer")
    cleanup = _member_body(quest, "fragment_stage_9000_item_00")

    assert "playerRef.SetValue(pBoS03StartedAV, 1.0)" in start
    assert "SetObjectiveDisplayed(100, True)" in start
    assert "playerRef.AddPerk(oBoS03UpdateTransponderPerk)" in begin_search
    assert "pBoS03_Transponders.Start()" in begin_search
    assert "pBoS03_Transponder_Radio.Start()" in begin_search
    assert "Alias_Transponder05.GetReference()" in checkpoint
    assert "playerRef.SetValue(pBoS03_CheckpointValue, 1.0)" in checkpoint
    assert "pCheckpointMessage.Show()" in checkpoint
    assert "AddItem(" not in checkpoint
    assert "SetObjectiveCompleted(200, True)" in final_transponder
    assert "SetObjectiveDisplayed(300, True)" in final_transponder
    assert "Alias_PlayerHasAccess.ForceRefTo(playerRef)" in access
    assert "playerRef.RemovePerk(oBoS03UpdateTransponderPerk)" in cleanup
    assert "pBoS03_Transponders.Stop()" in cleanup
    assert "Alias_CurrentTransponder.Clear()" in cleanup

    assert "SetObjectiveCompleted(500, True)" in end
    assert "playerRef.SetValue(pBoS03CompletedAV, 1.0)" in end
    assert "CompleteQuest()" in end
    assert "BoS03_TryStartEN01()" in end
    assert "StartTimer(5.0, 600)" in end
    assert (
        'Game.GetFormFromFile(0x000649C5, "SeventySix.esm") as Quest'
        in handoff_helper
    )
    assert "en01Quest.IsRunning() || en01Quest.IsCompleted()" in handoff_helper
    handoff = (
        "pEN01_MiscQuestStartKeyword.SendStoryEventAndWait("
        "None, playerRef, playerRef)"
    )
    assert handoff_helper.count(handoff) == 1
    assert "Return accepted ||" in handoff_helper
    assert "aiTimerID != 600" in retry
    assert "GetStageDone(700)" in retry
    assert "BoS03_TryStartEN01()" in retry
    assert "StartTimer(5.0, 600)" in retry
    assert "SetStage(700)" in end
    assert "AddItem(" not in end
