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
SCRIPT_NAME = "Fragments:Quests:QF_BS01_MQ06A_Raiders_005D2AFF"
HEALTH_SCRIPT_NAME = "Quests:_Default:SetStageOnHealthPercent"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
PIERCE_HEALTH_PERCENT = 0.0
PIERCE_STAGE_TO_SET = 660
EXPECTED_FRAGMENT_MEMBERS = {
    "fragment_stage_0010_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0350_item_00",
    "fragment_stage_0510_item_00",
    "fragment_stage_0520_item_00",
    "fragment_stage_0530_item_00",
    "fragment_stage_0540_item_00",
    "fragment_stage_0550_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0650_item_00",
    "fragment_stage_0655_item_00",
    "fragment_stage_0660_item_00",
    "fragment_stage_0665_item_00",
    "fragment_stage_0670_item_00",
    "fragment_stage_0680_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_0800_item_00",
    "fragment_stage_0825_item_00",
    "fragment_stage_0850_item_00",
    "fragment_stage_0900_item_00",
    "fragment_stage_0950_item_00",
    "fragment_stage_1000_item_00",
    "fragment_stage_1050_item_00",
    "fragment_stage_1100_item_00",
    "fragment_stage_1150_item_00",
    "fragment_stage_1190_item_00",
    "fragment_stage_1200_item_00",
    "fragment_stage_1210_item_00",
    "fragment_stage_1250_item_00",
    "fragment_stage_1280_item_00",
    "fragment_stage_1290_item_00",
    "fragment_stage_1300_item_00",
    "fragment_stage_9000_item_00",
}
EXPECTED_CONTROL_MEMBERS = {"retrysettlershandoff", "ontimer"}
EXPECTED_MEMBERS = EXPECTED_FRAGMENT_MEMBERS | EXPECTED_CONTROL_MEMBERS


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


def _patch() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def _merged_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch()
    )


def _health_patch() -> str:
    patch = _script_patch_source(HEALTH_SCRIPT_NAME)
    assert patch is not None
    return patch


def _health_merged_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(HEALTH_SCRIPT_NAME, ".psc")
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _health_patch()
    )


def test_bs01_mq05_patch_is_member_only_and_contains_every_binding_once():
    patch = _patch()
    member_names = _member_names(patch)
    fragment_names = [
        member for member in member_names if member.startswith("fragment_stage_")
    ]

    assert len(fragment_names) == 34
    assert set(fragment_names) == EXPECTED_FRAGMENT_MEMBERS
    assert all(
        fragment_names.count(member) == 1 for member in EXPECTED_FRAGMENT_MEMBERS
    )
    assert set(member_names) == EXPECTED_MEMBERS
    assert all(member_names.count(member) == 1 for member in EXPECTED_MEMBERS)
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


def test_bs01_mq05_patch_merges_uniquely_and_idempotently():
    patch = _patch()
    merged = _merged_source()
    merged_names = _member_names(merged)

    assert "Scriptname Fragments:Quests:QF_BS01_MQ06A_Raiders_005D2AFF" in merged
    assert merged.count(" Property ") == 30
    for member in EXPECTED_MEMBERS:
        assert merged_names.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged


def test_health_percent_helper_uses_exact_member_only_event_contract():
    patch = _health_patch()
    members = [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ]
    on_hit = _member_body(patch, "onhit")

    assert members == [
        ("event", "onaliasinit"),
        ("event", "onhit"),
        ("event", "onenterbleedout"),
        ("function", "applyreachedstages"),
    ]
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )
    assert on_hit.startswith(
        "Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, "
        "Form akSource, Projectile akProjectile, bool abPowerAttack, \\\n"
        "\tBool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, "
        "String apMaterial)"
    )


def test_health_percent_helper_merges_uniquely_and_idempotently():
    patch = _health_patch()
    merged = _health_merged_source()
    merged_names = _member_names(merged)

    assert "Scriptname Quests:_Default:SetStageOnHealthPercent" in merged
    assert merged.count("Struct StageDatum") == 1
    assert merged.count("StageDatum[] Property StageData Auto mandatory") == 1
    assert (
        "Struct StageDatum\n\tFloat HealthPercent\n\tInt StageToSet\nEndStruct"
        in merged
    )
    assert merged_names.count("onaliasinit") == 1
    assert merged_names.count("onhit") == 1
    assert merged_names.count("onenterbleedout") == 1
    assert merged_names.count("applyreachedstages") == 1
    assert _member_body(merged, "onaliasinit") == _member_body(
        patch, "onaliasinit"
    )
    assert _member_body(merged, "onhit") == _member_body(patch, "onhit")
    assert _member_body(merged, "onenterbleedout") == _member_body(
        patch, "onenterbleedout"
    )
    assert _member_body(merged, "applyreachedstages") == _member_body(
        patch, "applyreachedstages"
    )
    assert _merge_script_method_patches(merged, patch) == merged


def test_health_percent_helper_accepts_non_player_hits_and_is_actor_safe():
    patch = _health_patch()
    on_init = _member_body(patch, "onaliasinit")
    on_hit = _member_body(patch, "onhit")

    assert on_init.count("RegisterForHitEvent(Self)") == 1
    assert "Actor targetActor = GetActorReference()" in on_hit
    assert "targetActor != None" in on_hit
    assert "akTarget == targetActor" in on_hit
    assert "Game.GetPlayer()" not in on_hit
    assert "akAggressor ==" not in on_hit
    assert "owningQuest != None && owningQuest.IsRunning()" in on_hit
    assert on_hit.count("RegisterForHitEvent(Self)") == 1


def test_health_percent_helper_applies_each_reached_unset_bound_stage():
    patch = _health_patch()
    on_hit = _member_body(patch, "onhit")
    apply_stages = _member_body(patch, "applyreachedstages")

    assert "targetActor.GetValuePercentage(Game.GetHealthAV())" in on_hit
    assert "owningQuest == None || !owningQuest.IsRunning()" in apply_stages
    assert "StageData == None" in apply_stages
    assert "While index < StageData.Length" in apply_stages
    assert "StageDatum stageDatumToCheck = StageData[index]" in apply_stages
    assert (
        "targetHealthPercent <= stageDatumToCheck.HealthPercent" in apply_stages
    )
    assert (
        "!owningQuest.IsStageDone(stageDatumToCheck.StageToSet)"
        in apply_stages
    )
    assert (
        apply_stages.count("owningQuest.SetStage(stageDatumToCheck.StageToSet)")
        == 1
    )


def test_health_percent_helper_guarantees_zero_threshold_on_bleedout_once():
    patch = _health_patch()
    bleedout = _member_body(patch, "onenterbleedout")
    apply_stages = _member_body(patch, "applyreachedstages")

    assert bleedout.startswith("Event OnEnterBleedout()")
    assert "GetActorReference() != None" in bleedout
    assert bleedout.count("ApplyReachedStages(0.0)") == 1
    assert "!owningQuest.IsStageDone(stageDatumToCheck.StageToSet)" in apply_stages
    assert (
        apply_stages.count("owningQuest.SetStage(stageDatumToCheck.StageToSet)")
        == 1
    )


@pytest.mark.parametrize(
    ("stage", "value"),
    ((510, 1), (520, 2), (530, 3), (540, 4), (550, 5)),
)
def test_bs01_mq05_negotiation_decisions_persist_on_local_player(
    stage: int, value: int
):
    body = _member_body(_patch(), f"fragment_stage_{stage:04d}_item_00")

    assert "Alias_Player.GetActorReference()" in body
    assert (
        f"player.SetValue(BS01_MQ05_Raiders_AV_Negotiation, {value}.0)"
        in body
    )


def test_bs01_mq05_combat_and_death_stages_converge_once():
    patch = _patch()
    combat = _member_body(patch, "fragment_stage_0650_item_00")
    raiders_dead = _member_body(patch, "fragment_stage_0655_item_00")
    pierce_downed = _member_body(patch, "fragment_stage_0660_item_00")
    interrogation_ready = _member_body(patch, "fragment_stage_0665_item_00")
    health_patch = _health_patch()
    health_helper = _member_body(health_patch, "applyreachedstages")
    bleedout = _member_body(health_patch, "onenterbleedout")

    assert "RemoveFromFaction(CaptiveFaction)" in combat
    assert "AddToFaction(BS01_MQ05_Raiders_EnemyFaction)" in combat
    assert "StartCombat(shin, True)" in combat
    assert "IsStageDone(660) && !IsStageDone(665)" in raiders_dead
    assert raiders_dead.count("SetStage(665)") == 1
    assert "IsStageDone(655) && !IsStageDone(665)" in pierce_downed
    assert pierce_downed.count("SetStage(665)") == 1
    assert PIERCE_HEALTH_PERCENT == 0.0
    assert PIERCE_STAGE_TO_SET == 660
    assert "ApplyReachedStages(0.0)" in bleedout
    assert "targetHealthPercent <= stageDatumToCheck.HealthPercent" in health_helper
    assert "owningQuest.SetStage(stageDatumToCheck.StageToSet)" in health_helper
    assert "Alias_Marker_PierceDowned.GetReference()" in interrogation_ready
    assert "pierce.MoveTo(downedMarker)" in interrogation_ready
    assert "SetObjectiveDisplayed(55)" in interrogation_ready
    assert "BS01_MQ06A_Raiders_06_ShinAfterCombat.Start()" in interrogation_ready


def test_bs01_mq05_peace_and_combat_paths_release_scene_actors():
    patch = _patch()

    negotiated = _member_body(patch, "fragment_stage_0600_item_00")
    assert "Alias_Actor_Pierce_RaiderCave.GetActorReference()" in negotiated
    assert "Alias_Actors_RaiderCrew.GetAt(index)" in negotiated
    assert negotiated.count("EvaluatePackage()") == 2

    interrogation_done = _member_body(patch, "fragment_stage_0680_item_00")
    assert "SetObjectiveCompleted(55)" in interrogation_done
    assert "Alias_Actor_Shin_RaiderCave.GetActorReference()" in interrogation_done
    assert "shin.EvaluatePackage()" in interrogation_done

    resolved = _member_body(patch, "fragment_stage_0700_item_00")
    assert "SetObjectiveCompleted(20)" in resolved
    assert "SetObjectiveDisplayed(60)" in resolved
    assert "shin.MoveTo(relogMarker)" in resolved


def test_bs01_mq05_objectives_scenes_and_social_state_follow_walkthrough():
    patch = _patch()

    assert "SetObjectiveDisplayed(10)" in _member_body(
        patch, "fragment_stage_0100_item_00"
    )
    assignment = _member_body(patch, "fragment_stage_0200_item_00")
    assert assignment.index("SetObjectiveCompleted(10)") < assignment.index(
        "SetObjectiveDisplayed(20)"
    )
    assert "MakeshiftVaultMapMarker.AddToMap()" in assignment

    cave = _member_body(patch, "fragment_stage_0350_item_00")
    assert "enableMarker.Enable()" in cave
    assert "BS01_MQ06A_Raiders_05_Negotiation.Start()" in cave

    crater = _member_body(patch, "fragment_stage_0800_item_00")
    assert "player.SetValue(BS01_ShinAwayValue, 0.0)" in crater
    assert "player.SetValue(BS01_PierceAwayValue, 0.0)" in crater
    assert crater.index("SetObjectiveCompleted(60)") < crater.index(
        "SetObjectiveDisplayed(70)"
    )

    war_room = _member_body(patch, "fragment_stage_0825_item_00")
    assert "BS01_MQ05_Raiders_WarRoomAmbient.Start()" in war_room
    pierce = _member_body(patch, "fragment_stage_0900_item_00")
    assert "player.SetValue(BS01_AV_MetPierce, 1.0)" in pierce
    assert pierce.index("SetObjectiveCompleted(70)") < pierce.index(
        "SetObjectiveDisplayed(80)"
    )


def test_bs01_mq05_real_and_dummy_holotape_paths_remain_distinct():
    patch = _patch()
    sheena_deal = _member_body(patch, "fragment_stage_1000_item_00")
    valdez = _member_body(patch, "fragment_stage_1050_item_00")
    real_data = _member_body(patch, "fragment_stage_1100_item_00")
    dummy_data = _member_body(patch, "fragment_stage_1150_item_00")
    uploaded = _member_body(patch, "fragment_stage_1190_item_00")
    returned = _member_body(patch, "fragment_stage_1200_item_00")

    assert "player.PlaceAtMe(BS01_MQ06A_Raiders_SheenasHolotape" in sheena_deal
    assert "Alias_QO_Holotape_Sheenas.ForceRefTo(holotapeRef)" in sheena_deal
    assert "player.AddItem(holotapeRef, 1, True)" in sheena_deal
    assert "player.AddItem(BS01_Valdez_Terminal_Password, 1, True)" in valdez
    assert "SetObjectiveCompleted(100)" in valdez

    assert "player.SetValue(BS01_MQ05_Raiders_AV_HelpedSheena, 1.0)" in real_data
    assert "player.SetValue(BS01_MQ05_Raiders_AV_HelpedSheena, 0.0)" in dummy_data
    for body in (real_data, dummy_data):
        assert "!IsStageDone(1190)" in body
        assert body.count("SetStage(1190)") == 1

    assert "SetObjectiveCompleted(90)" in uploaded
    assert "SetObjectiveDisplayed(110)" in uploaded
    assert "player.RemoveItem(holotapeRef, 1, True)" in returned
    assert "player.RemoveItem(BS01_MQ06A_Raiders_SheenasHolotape" in returned
    assert returned.index("SetObjectiveCompleted(110)") < returned.index(
        "SetObjectiveDisplayed(120)"
    )


def test_bs01_mq05_dual_report_and_terminal_handoff_are_guarded():
    patch = _patch()
    raiders_report = _member_body(patch, "fragment_stage_1280_item_00")
    settlers_report = _member_body(patch, "fragment_stage_1290_item_00")
    reports_complete = _member_body(patch, "fragment_stage_1300_item_00")
    terminal = _member_body(patch, "fragment_stage_9000_item_00")
    handoff = _member_body(patch, "retrysettlershandoff")
    timer = _member_body(patch, "ontimer")

    assert "IsStageDone(1290) && !IsStageDone(1300)" in raiders_report
    assert raiders_report.count("SetStage(1300)") == 1
    assert "IsStageDone(1280) && !IsStageDone(1300)" in settlers_report
    assert settlers_report.count("SetStage(1300)") == 1
    assert "SetObjectiveDisplayed(120)" in reports_complete
    assert "SetStage(9000)" not in reports_complete

    send = (
        "BS01_MQ06B_Settlers_QuestStartKeyword."
        "SendStoryEventAndWait(None, player, player)"
    )
    successor_state = (
        "BS01_MQ06B_Settlers.IsRunning() || "
        "BS01_MQ06B_Settlers.IsCompleted()"
    )
    accepted_or_retry = (
        "If handoffAccepted\n"
        "        Stop()\n"
        "    Else\n"
        "        StartTimer(5.0, 9000)\n"
        "    EndIf"
    )

    assert terminal.index("SetObjectiveCompleted(120)") < terminal.index(
        "RetrySettlersHandoff()"
    )
    assert send not in terminal
    assert "Stop()" not in terminal
    assert "StartTimer(" not in terminal

    assert "Bool handoffAccepted = False" in handoff
    assert handoff.count(successor_state) == 2
    assert handoff.count(f"handoffAccepted = {send}") == 1
    assert handoff.index(successor_state) < handoff.index(send)
    assert handoff.index(send) < handoff.rindex(successor_state)
    assert handoff.rindex(successor_state) < handoff.index(
        "If handoffAccepted"
    )
    assert accepted_or_retry in handoff
    assert handoff.count("Stop()") == 1
    assert handoff.count("StartTimer(5.0, 9000)") == 1
    assert ".Start()" not in handoff
    assert ".SendStoryEvent(" not in handoff

    assert timer.startswith("Event OnTimer(Int aiTimerID)")
    assert (
        "aiTimerID == 9000 && IsRunning() && IsStageDone(9000)" in timer
    )
    assert timer.count("RetrySettlersHandoff()") == 1
    assert "Stop()" not in timer
    assert "StartTimer(" not in timer


def test_bs01_mq05_full_production_merge_native_compiles_for_fo4():
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


def test_health_percent_helper_full_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _health_merged_source(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(HEALTH_SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
