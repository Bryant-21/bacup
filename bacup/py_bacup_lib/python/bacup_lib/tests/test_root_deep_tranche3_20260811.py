from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
REPAIRS = (
    "COMP_CommentTriggerScript.psc",
    "E09B_ButtonScript.psc",
    "MoMParlorEntryTriggerScript.psc",
    "MTR05_SayOnActivate.psc",
    "MTR10ShutdownButtonScript.psc",
    "SFL02_Track_TriggerAliasScript.psc",
    "TopOfTheWorldFloor3ButtonScript.psc",
    "VaultDefaultDestMultiStateActivator.psc",
)


def _merged(relative_path: str) -> str:
    skeleton = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    patch = _script_patch_source(relative_path.removesuffix(".psc"))
    assert patch is not None
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


@pytest.mark.parametrize("relative_path", REPAIRS)
def test_tranche3_root_repairs_merge_idempotently_and_compile(relative_path: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        _merged(relative_path),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=relative_path,
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_companion_comment_uses_bound_owner_speaker_and_repeat_policy():
    merged = _merged("COMP_CommentTriggerScript.psc")
    assert "TriggerOnce && triggered" in merged
    assert "commentQuest.Speaker.GetActorReference()" in merged
    assert "speakerRef.SayCustom(SubtypeToSay, None, False, playerRef)" in merged


def test_sfl02_trigger_balances_player_collection_and_perk_lifecycle():
    merged = _merged("SFL02_Track_TriggerAliasScript.psc")
    assert "SFL02VertibotPlayers.AddRef(playerRef)" in merged
    assert "playerRef.AddPerk(SFL02_Track_VertibotAttachPerk)" in merged
    assert "SFL02VertibotPlayers.RemoveRef(playerRef)" in merged
    assert "playerRef.RemovePerk(SFL02_Track_VertibotAttachPerk)" in merged


def test_mom_parlor_entry_selects_a_bounded_rank_line_once():
    merged = _merged("MoMParlorEntryTriggerScript.psc")
    assert "MoM00.GetStage() < CONST_MoM00_QuestCompleted" in merged
    assert "rankIndex >= MomParlorEntryTopics.Length" in merged
    assert "playerRef.SetValue(MoMParlorEntryLinePlayed, 1.0)" in merged
    assert "MoMCryptosVoiceF_VOICEONLY.Say(MomParlorEntryTopics[rankIndex]" in merged


def test_floor_three_button_requires_the_signal_or_plays_the_bound_failure_line():
    merged = _merged("TopOfTheWorldFloor3ButtonScript.psc")
    assert "playerRef.HasKeyword(MTNS01_SignalBoosted_Keyword)" in merged
    assert "GetLinkedRef(LinkCustom01)" in merged
    assert "Loudspeaker.Say(Dialogue_RDR_Rose_Button3FailTopic" in merged


def test_master_shutdown_button_completes_its_owning_quest_once():
    merged = _merged("MTR10ShutdownButtonScript.psc")
    assert 'GoToState("ready")' in merged
    assert 'GoToState("busy")' in merged
    assert "GetOwningQuest().SetStage(200)" in merged


def test_spin_button_advances_only_the_prompted_wheel_and_resets_on_shutdown():
    merged = _merged("E09B_ButtonScript.psc")
    assert "GetOwningQuest().GetStage() != 160" in merged
    assert 'buttonRef.PlayAnimation("TurnOn01")' in merged
    assert "GetOwningQuest().SetStage(170)" in merged
    assert 'buttonRef.PlayAnimation("TurnOff01")' in merged


def test_destroyable_multistate_uses_bound_state_names_for_destroy_and_repair():
    merged = _merged("VaultDefaultDestMultiStateActivator.psc")
    assert "FindAnimationStateIndex(DestroyedStateName)" in merged
    assert "repairToStateName = ShouldRepairToFixedStateName" in merged
    assert "repairToStateName = UnusedStateName" in merged
    assert "SetLocalState(repairedIndex, True)" in merged


def test_hornwright_printer_selects_a_bound_response_and_throttles_audio():
    merged = _merged("MTR05_SayOnActivate.psc")
    assert "!MTR_Hornwright_Industrial_Master.IsRunning()" in merged
    assert "!MTR05_Mother.IsRunning()" in merged
    assert "MTR05_Mother.GetStage() >= 30" in merged
    assert "MTR05_Mother.SetStage(50)" in merged
    assert "responseTopic = MTR05_IDPrinterInvalidUser" in merged
    assert "playerRef.HasKeyword(MTR05_Main_PlayerIsExecKeyword)" in merged
    assert "responseTopic = MTR05_CardIssued" in merged
    assert "playerRef.GetItemCount(MTR05_HRPassword) > 0" in merged
    assert "StartTimer(iAudioCooldownLength as Float, iAudioCooldownID)" in merged
    assert 'GoToState("ready")' in merged
