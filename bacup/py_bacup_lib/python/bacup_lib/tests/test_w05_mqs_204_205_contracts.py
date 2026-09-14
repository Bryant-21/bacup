from __future__ import annotations

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


QUEST_FRAGMENT = "Fragments:Quests:QF_W05_MQS_204P_0040C458"
MOTHERLODE = "W05_MQS_204P_MotherlodeScript"
MOTHERLODE_SKELETON = """Scriptname W05_MQS_204P_MotherlodeScript Extends ReferenceAlias

message Property W05_MQS_204P_MotherlodeMSG Auto mandatory
actorvalue Property W05_MQS_204P_MotherlodeFinalSetting Auto mandatory
Int Property StageToSet Auto mandatory
ReferenceAlias Property CurrentPlayer Auto mandatory

Event OnActivate(ObjectReference akActionRef)
EndEvent
"""


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _merged_motherlode() -> str:
    patch = _script_patch_source(MOTHERLODE)
    assert patch is not None
    return _merge_script_method_patches(MOTHERLODE_SKELETON, patch)


def test_settler_stage_9000_records_settler_choice_before_handoff():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    body = _member_body(patch, "Fragment_Stage_9000_Item_00")
    faction_choice = "playerRef.SetValue(W05_MQ_204P_FactionChosen, 2.0)"
    handoff = "W05_MQS_205P_QuestStartKeyword.SendStoryEvent"
    assert faction_choice in body
    assert "W05_MQ_204P_FactionChosen, 1.0" not in body
    assert body.index(faction_choice) < body.index(handoff)


def test_paige_broadcast_stage_starts_the_bound_local_scene():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    body = _member_body(patch, "Fragment_Stage_0100_Item_00")
    assert "SetObjectiveCompleted(15)" in body
    assert "Alias_Paige.GetActorReference()" in body
    assert "paigeRef.EvaluatePackage()" in body
    assert "W05_MQS_204P_PaigeReactToHijackScene.Start()" in body


def test_motherlode_activation_requires_player_confirmation_and_sets_stage_once():
    patch = _script_patch_source(MOTHERLODE)
    assert patch is not None
    merged = _merged_motherlode()
    body = _member_body(merged, "OnActivate")
    assert "playerRef = CurrentPlayer.GetActorReference()" in body
    assert "playerRef == None || akActionRef != playerRef" in body
    assert "W05_MQS_204P_MotherlodeMSG.Show() != 1" in body
    assert "owningQuest.IsStageDone(StageToSet)" in body
    assert "MotherlodeAggressiveChoicesMade >= 3" in body
    assert "MotherlodeEmpatheticChoicesMade >= 3" in body
    assert body.index(
        "playerRef.SetValue(W05_MQS_204P_MotherlodeFinalSetting"
    ) < body.index("owningQuest.SetStage(StageToSet)")
    assert merged.lower().count("event onactivate(") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_motherlode_full_merge_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        _merged_motherlode(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="W05_MQS_204P_MotherlodeScript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_maze_receivers_activate_each_bound_animated_barrier_and_controllers_merge():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    for stage, index in zip(
        ("0615", "0625", "0635", "0645", "0655"),
        ("01", "02", "03", "04", "05"),
        strict=True,
    ):
        body = _member_body(patch, f"Fragment_Stage_{stage}_Item_00")
        assert f"Alias_FakeBarrier{index}.GetReference()" in body
        assert "Actor playerRef = Game.GetPlayer()" in body
        assert "wallRef.Activate(playerRef)" in body
        assert ".Disable()" not in body

    member_names = {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert len(member_names) == 36
    assert "fragment_stage_0015_item_00" in member_names
    assert "fragment_stage_1100_item_00" in member_names
    fake_wall = _script_patch_source("W05_MQS_204P_FakeWallScript")
    player = _script_patch_source("W05_MQS_204P_PlayerScript")
    assert fake_wall is not None
    assert player is not None
    assert "akActionRef != playerRef" in fake_wall
    assert "owningQuest.SetStage(StageToSet)" in fake_wall
    assert "akFurniture != scannerRef" in player
    assert "owningQuest.SetStage(StageToSet)" in player


def test_maze_progression_has_no_forced_stage_or_inventory_shortcuts():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    assert "SetStage(250)" not in _member_body(
        patch, "Fragment_Stage_0200_Item_00"
    )
    assert "SetStage(700)" not in _member_body(
        patch, "Fragment_Stage_0600_Item_00"
    )
    assert "SetStage(800)" not in _member_body(
        patch, "Fragment_Stage_0700_Item_00"
    )
    module_stage = _member_body(patch, "Fragment_Stage_0800_Item_00")
    assert "Alias_ModuleContainer.GetReference()" in module_stage
    assert "moduleContainer.GetItemCount(W05_MQS_204P_IntelligenceModule) < 1" in module_stage
    assert "moduleContainer.AddItem(W05_MQS_204P_IntelligenceModule, 1, False)" in module_stage


def test_all_that_glitters_shutdown_strip_has_a_contract_note() -> None:
    """WK145 `0041CB6D` lost a propertyless `DefaultQuestShutdownScript` binding.

    The strip is a deliberate, gated Rust hook with two pinning Rust tests; the
    Wave 1 obligation was the missing contract note plus one check that the
    quest still terminates through its own completion stage.
    """
    from pathlib import Path as _Path

    contract = (
        _Path(__file__).resolve().parents[5]
        / "bacup"
        / "docs"
        / "stub_restoration"
        / "contracts"
        / "w05-mqs-205-shutdown-strip-2026-09-09.md"
    ).read_text(encoding="utf-8")

    for token in (
        "strip_w05_mqs_205_shutdown_script_binding",
        "`0x0041_CB6D`",
        "pre_translate_strips_only_w05_mqs_205_root_shutdown_script_binding",
        "pre_translate_keeps_matching_shutdown_script_on_other_plugins",
        "no active players",
        "**25**",
        "**9000**",
        "**10000**",
        "33 / 33",
        "It is not a defect",
    ):
        assert token in contract, token

