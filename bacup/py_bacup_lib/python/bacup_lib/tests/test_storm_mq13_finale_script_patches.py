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
PART_ONE_FRAGMENT = "Fragments:Quests:QF_Storm_MQ13_Finale_00745597"
PART_TWO_FRAGMENT = "Fragments:Quests:QF_Storm_MQ13_Finale_Pt2_0075A8F1"
PART_ONE_CONTROLLER = "Quests:Storm:MQ13:Part1QuestScript"
GAUNTLET_CONTROLLER = "Quests:Storm:MQ13:LostGauntletScript"
FINALE_CONTROLLER = "Quests:Storm:MQ13:QuestScript"
HUGO_CONTROLLER = "Quests:Storm:MQ13:HugoAliasScript"
WEATHER_LAB_MOVE = "Quests:Storm:MQ13:WeatherLabMoveScript"
PATCHED_SCRIPTS = (
    PART_ONE_FRAGMENT,
    PART_TWO_FRAGMENT,
    PART_ONE_CONTROLLER,
    GAUNTLET_CONTROLLER,
    FINALE_CONTROLLER,
    HUGO_CONTROLLER,
    WEATHER_LAB_MOVE,
)

DECLARED_FRAGMENT_STAGES = {
    PART_ONE_FRAGMENT: {
        0, 100, 110, 115, 200, 210, 220, 225, 300, 310, 320, 325,
        400, 410, 420, 425, 500, 9000, 9990, 10000,
    },
    PART_TWO_FRAGMENT: {
        10, 100, 110, 120, 200, 205, 210, 300, 350, 400, 450, 500,
        510, 520, 600, 610, 620, 625, 630, 700, 710, 750, 800, 900,
        9000, 9990, 10000,
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
def test_finale_patch_is_member_only(script_name: str):
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
def test_finale_production_merge_is_unique_and_idempotent(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    merged_names = _member_names(merged)

    for member_name in _member_names(patch):
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", DECLARED_FRAGMENT_STAGES)
def test_finale_fragments_only_implement_declared_stages(script_name: str):
    implemented = {
        int(name[len("fragment_stage_") :].split("_", 1)[0])
        for name in _member_names(_script_patch_source(script_name))
        if name.startswith("fragment_stage_")
    }
    assert implemented <= DECLARED_FRAGMENT_STAGES[script_name]


def test_gathering_clouds_reconstructs_three_timed_defenses_and_story_handoff():
    fragment = _script_patch_source(PART_ONE_FRAGMENT)
    controller = _script_patch_source(PART_ONE_CONTROLLER)
    for activation_stage, completion_stage, wave_index in (
        (210, 220, 0),
        (310, 320, 1),
        (410, 420, 2),
    ):
        body = _member_body(
            fragment, f"fragment_stage_{activation_stage:04d}_item_00"
        )
        assert f"StartHarvesterDefense({wave_index}, {activation_stage}, {completion_stage})" in body
        assert f"FinishHarvesterDefense({activation_stage}, {completion_stage})" in controller
    assert "StartTimer(45.0, aiCompletionStage)" in controller
    assert "StartLocalEncounterWave(aiWaveIndex)" in controller

    completion = _member_body(fragment, "fragment_stage_9000_item_00")
    assert "Storm_MQ13_Finale_Pt2_StartKeyword.SendStoryEvent()" in completion
    assert ".Start()" not in completion
    assert "SetStage(" not in completion
    assert "AddItem(" not in completion


def test_quest_sibling_casts_hop_through_quest_ancestor():
    part_one_fragment = _script_patch_source(PART_ONE_FRAGMENT)
    part_two_fragment = _script_patch_source(PART_TWO_FRAGMENT)
    part_one_controller = _script_patch_source(PART_ONE_CONTROLLER)
    gauntlet_controller = _script_patch_source(GAUNTLET_CONTROLLER)
    finale_controller = _script_patch_source(FINALE_CONTROLLER)

    assert "(Self as Quest) as Quests:Storm:MQ13:Part1QuestScript" in part_one_fragment
    assert "(Self as Quest) as Quests:Storm:MQ13:LostGauntletScript" in part_two_fragment
    assert "(Self as Quest) as DefaultQuestEncounterWaveScript" in part_one_controller
    assert gauntlet_controller.count(
        "(Self as Quest) as DefaultQuestEncounterWaveScript"
    ) == 2
    assert "(Self as Quest) as Quests:Storm:MQ13:QuestScript" in part_two_fragment
    assert "(Self as Quest) as Quests:Storm:MQ13:LostGauntletScript" in finale_controller
    assert "Self as Quests:Storm:MQ13:Part1QuestScript" not in part_one_fragment
    assert "Self as Quests:Storm:MQ13:LostGauntletScript" not in part_two_fragment
    assert "Self as DefaultQuestEncounterWaveScript" not in part_one_controller
    assert "Self as DefaultQuestEncounterWaveScript" not in gauntlet_controller


def test_gathering_clouds_destroyed_harvesters_fail_forward_without_online_repair():
    fragment = _script_patch_source(PART_ONE_FRAGMENT)
    for destroyed_stage, activation_stage, completion_stage in (
        (225, 210, 220),
        (325, 310, 320),
        (425, 410, 420),
    ):
        body = _member_body(
            fragment, f"fragment_stage_{destroyed_stage:04d}_item_00"
        )
        assert f"FinishHarvesterDefense({activation_stage}, {completion_stage})" in body


def test_eye_of_storm_restores_gauntlet_three_choices_and_final_reporter():
    fragment = _script_patch_source(PART_TWO_FRAGMENT)
    gauntlet = _script_patch_source(GAUNTLET_CONTROLLER)
    for wave_index in range(2):
        assert f"StartGauntletWave({wave_index})" in fragment
    assert "StartLocalEncounterWave(aiWaveIndex)" in gauntlet
    for stage, choice in ((610, 1), (620, 2), (630, 3)):
        body = _member_body(fragment, f"fragment_stage_{stage:04d}_item_00")
        assert f"SetValue(Storm_MQ13_HugoChoice, {choice}.0)" in body
    assert "Alias_Actor_Hugo.GetReference()" in _member_body(
        fragment, "fragment_stage_0900_item_01"
    )
    assert "Alias_Actor_Audrey_Atrium.GetReference()" in _member_body(
        fragment, "fragment_stage_0900_item_02"
    )
    assert "Alias_Actor_Oberlin.GetReference()" in _member_body(
        fragment, "fragment_stage_0900_item_03"
    )
    completion = _member_body(fragment, "fragment_stage_9000_item_00")
    assert "AddItem(" not in completion
    assert "SendStoryEvent" not in completion


def test_eye_of_storm_orders_boss_add_clone_and_final_solo_phases():
    fragment = _script_patch_source(PART_TWO_FRAGMENT)
    controller = _script_patch_source(FINALE_CONTROLLER)

    stage_500 = _member_body(fragment, "fragment_stage_0500_item_01")
    stage_510 = _member_body(fragment, "fragment_stage_0510_item_00")
    stage_520 = _member_body(fragment, "fragment_stage_0520_item_00")
    assert "BeginBossAddPhase()" in stage_500
    assert "BeginClonePhase()" in stage_510
    assert "BeginFinalSoloPhase()" in stage_520

    boss_adds = _member_body(controller, "beginbossaddphase")
    clones = _member_body(controller, "beginclonephase")
    final_solo = _member_body(controller, "beginfinalsolophase")
    assert "StartGauntletWave(2)" in boss_adds
    assert "StartGauntletWave(3)" not in boss_adds
    assert clones.index("ClearEncounterCollection(Alias_LostMobs)") < clones.index(
        "StartGauntletWave(3)"
    )
    assert "StartGauntletWave(" not in final_solo
    assert "ClearEncounterCollection(Alias_HugoClones)" in final_solo


def test_eye_of_storm_reentry_recovers_finished_fight_before_choice():
    stage_10 = _member_body(
        _script_patch_source(PART_TWO_FRAGMENT), "fragment_stage_0010_item_00"
    )
    assert "IsStageDone(600) && !IsStageDone(700)" in stage_10
    assert stage_10.index("EndBossFight()") < stage_10.index("SetStage(700)")


def test_boss_arcs_run_only_during_combat_and_stage_600_cleans_encounters():
    fragment = _script_patch_source(PART_TWO_FRAGMENT)
    controller = _script_patch_source(FINALE_CONTROLLER)
    start_arcs = _member_body(controller, "startelectricarcs")
    arc_timer = _member_body(controller, "ontimer")
    stage_600 = _member_body(fragment, "fragment_stage_0600_item_00")
    assert "IsStageDone(500) && !IsStageDone(600)" in start_arcs
    assert "!IsStageDone(500) || IsStageDone(600)" in arc_timer
    assert stage_600.index("EndBossFight()") < stage_600.index(
        "SetObjectiveCompleted(40)"
    )


def test_capture_uses_bound_fade_furniture_link_and_stage_700_guard():
    fragment = _script_patch_source(PART_TWO_FRAGMENT)
    controller = _script_patch_source(FINALE_CONTROLLER)
    assert "CaptureHugo()" in _member_body(
        fragment, "fragment_stage_0700_item_03"
    )
    capture = _member_body(controller, "capturehugo")
    assert "!IsStageDone(iCaptureHugoStage)" in capture
    assert "FadeToBlackSpell.Cast(player, player)" in capture
    assert "hugo.GetLinkedRef(LinkCustom01)" in capture
    assert "hugo.PlaceAtMe(CaptiveFurniture" in capture
    assert "captiveFurnitureRef.Activate(hugo)" in capture


def test_completion_runs_stage_10000_cleanup_then_stops_without_duplicate_rewards():
    fragment = _script_patch_source(PART_TWO_FRAGMENT)
    completion = _member_body(fragment, "fragment_stage_9000_item_00")
    cleanup = _member_body(fragment, "fragment_stage_10000_item_00")
    assert completion.index("CompleteAllObjectives()") < completion.index(
        "SetStage(10000)"
    ) < completion.index("Stop()")
    assert "AddItem(" not in completion
    assert "ShutdownFinale()" in cleanup
    shutdown = _member_body(_script_patch_source(FINALE_CONTROLLER), "onquestshutdown")
    assert "ShutdownFinale()" in shutdown


def test_both_finale_halves_fail_stage_reverts_state_without_completing():
    for script_name, expected in (
        (
            PART_ONE_FRAGMENT,
            (
                "SetHarvesterActive(Alias_Ref_LightningHarvesterA, False)",
                "SetHarvesterActive(Alias_Ref_LightningHarvesterB, False)",
                "SetHarvesterActive(Alias_Ref_LightningHarvesterC, False)",
            ),
        ),
        (PART_TWO_FRAGMENT, ("ShutdownFinale()",)),
    ):
        failure = _member_body(
            _script_patch_source(script_name), "fragment_stage_9990_item_00"
        )
        assert "FailAllObjectives()" in failure
        for fragment_text in expected:
            assert fragment_text in failure
        assert "CompleteAllObjectives(" not in failure
        assert "SetObjectiveCompleted(" not in failure
        assert "SendStoryEvent" not in failure
        assert "SetStage(" not in failure
        assert "AddItem(" not in failure


def test_hugo_boss_advances_each_bound_phase_once_and_stops_after_final_phase():
    source = _script_patch_source(HUGO_CONTROLLER)
    bleedout = _member_body(source, "onenterbleedout")
    assert "iCurrentFightStage += 1" in bleedout
    assert "iFightStageArray[iCurrentFightStage - 1]" in bleedout
    assert "!MQ13QuestScript.IsStageDone(stageToSet)" in bleedout
    assert "iCurrentFightStage >= iFightStages" in bleedout
    assert "StartTimer(fRechargeAnimationLength, iHugoAnimationFailsafeTimerID)" in bleedout
    recharge = _member_body(source, "finishrecharge")
    assert "ResetHealthAndLimbs()" in recharge
    assert "ApplyFightStageLoadout(iCurrentFightStage)" in recharge


def test_weather_lab_reentry_move_is_player_stage_and_turnoff_guarded():
    trigger = _member_body(_script_patch_source(WEATHER_LAB_MOVE), "ontriggerenter")
    assert "akActionRef != Game.GetPlayer()" in trigger
    assert "!owningQuest.IsStageDone(iStageToMove)" in trigger
    assert "owningQuest.IsStageDone(iTurnOffStage)" in trigger
    assert "akActionRef.MoveTo(destination" in trigger


@pytest.mark.parametrize(
    "script_name",
    (
        "Quests:Storm:MQ13:ControlRoomTriggerCollectionScript",
        "Quests:Storm:MQ13:FadeToBlackEffectScript",
        "Quests:Storm:MQ13:weatherlablasergridtrigger",
    ),
)
def test_online_or_insufficiently_evidenced_shells_remain_unpatched(script_name: str):
    assert _script_patch_source(script_name) is None


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_finale_full_production_merge_native_compiles_for_fo4(script_name: str):
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
