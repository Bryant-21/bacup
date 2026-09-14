from __future__ import annotations

from collections import Counter
from pathlib import Path

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
QF_SCRIPT = "Fragments:Quests:QF_BS01_Invention_005B79EB"
QUEST_SCRIPT = "Quests:BS01_Invention:QuestScript"
PLAYER_SCRIPT = "Quests:BS01_Invention:PlayerScript"

EXPECTED_QF_MEMBERS = frozenset(
    f"fragment_stage_{stage}_item_00"
    for stage in (
        "0001",
        "0100",
        "0160",
        "0200",
        "0300",
        "0350",
        "0400",
        "0410",
        "0420",
        "0430",
        "0500",
        "0501",
        "0505",
        "0510",
        "0515",
        "0520",
        "0525",
        "0530",
        "0600",
        "0610",
        "0620",
        "0630",
        "0640",
        "0700",
        "0710",
        "0720",
        "0730",
        "0740",
        "0750",
        "0800",
        "0810",
        "0820",
        "0830",
        "0840",
        "0850",
        "0860",
        "0900",
        "0910",
        "0911",
        "0912",
        "0913",
        "0920",
        "0921",
        "0922",
        "0923",
        "0930",
        "0931",
        "0932",
        "0933",
        "1000",
        "1010",
        "1011",
        "1012",
        "1013",
        "1100",
        "1150",
        "1200",
        "1300",
        "1310",
        "1315",
        "1320",
        "9000",
        "10000",
    )
) | {"ontimer", "trystartfieldtesting"}
EXPECTED_MEMBERS = {
    QF_SCRIPT: EXPECTED_QF_MEMBERS,
    QUEST_SCRIPT: frozenset({"onstageset", "ontimer", "actor.ondeath"}),
    PLAYER_SCRIPT: frozenset({"onlocationchange"}),
}


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return source_path.read_text(encoding="utf-8")


def _merged(script_name: str) -> str:
    return _merge_script_method_patches(_source(script_name), _patch(script_name))


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _stage_body(stage: int) -> str:
    return _member_body(_patch(QF_SCRIPT), f"fragment_stage_{stage:04d}_item_00")


def test_all_bound_members_merge_exactly_once_and_preserve_declarations():
    for script_name, expected_members in EXPECTED_MEMBERS.items():
        patch = _patch(script_name)
        expected_counter = Counter({member: 1 for member in expected_members})
        assert Counter(_member_names(patch)) == expected_counter
        assert not any(
            line.strip().lower().startswith("scriptname ")
            for line in patch.splitlines()
        )

        source = _source(script_name)
        merged = _merged(script_name)
        assert Counter(_member_names(merged)) == expected_counter
        for declaration in source.splitlines():
            if declaration.strip():
                assert declaration in merged
        for member in expected_members:
            assert _member_body(merged, member) == _member_body(patch, member)
        assert _merge_script_method_patches(merged, patch) == merged


def test_diagnostics_wiring_and_inspection_choices_converge_locally():
    diagnostics = _stage_body(700)
    for objective in (800, 900, 1000):
        assert f"SetObjectiveDisplayed({objective})" in diagnostics

    assert "SetObjectiveCompleted(800)" in _stage_body(710)
    assert "BS01_Invention_Valdez_Diagnostics_Scene.Start()" in _stage_body(710)
    assert "valveRef.PlaceAtMe(ReleaseValveExplosion)" in _stage_body(720)
    assert "SetObjectiveCompleted(900)" in _stage_body(720)
    assert "BS01_Invention_Valdez_Wiring_Scene.Start()" in _stage_body(730)
    assert "SetStage(740)" in _stage_body(730)
    wiring_fight = _stage_body(740)
    assert 'Game.GetFormFromFile(0x000342C9, "Fallout4.esm") as ActorBase' in wiring_fight
    assert "RefCollectionAlias wiringEnemies = GetAlias(35) as RefCollectionAlias" in wiring_fight
    assert "While spawnIndex < 4" in wiring_fight
    assert "spawnPoint.PlaceActorAtMe(moleRatBase, 3)" in wiring_fight
    assert "wiringEnemies.AddRef(enemyRef)" in wiring_fight
    assert "enemyRef.StartCombat(playerRef)" in wiring_fight
    assert "SetStage(750)" not in wiring_fight
    assert "IsStageDone(710) && IsStageDone(720) && IsStageDone(730)" in _stage_body(750)
    assert "SetStage(800)" in _stage_body(750)

    convergence = (
        "(IsStageDone(810) || IsStageDone(820)) && "
        "(IsStageDone(830) || IsStageDone(840)) && "
        "(IsStageDone(850) || IsStageDone(860))"
    )
    for stage in (810, 830, 850):
        assert "BS01_Invention_ValdezRep_RaiseSmall_Global.GetValue()" in _stage_body(stage)
        assert convergence in _stage_body(stage)
        assert "SetStage(900)" in _stage_body(stage)
    for stage in (820, 840, 860):
        assert "BS01_Invention_ValdezRep_LowerSmall_Global.GetValue()" in _stage_body(stage)
        assert convergence in _stage_body(stage)
        assert "SetStage(900)" in _stage_body(stage)


def test_terminal_password_and_component_variants_use_only_bound_local_objects():
    terminal_password = _stage_body(430)
    assert "playerRef.GetItemCount(BS01_Valdez_Terminal_Password) == 0" in terminal_password
    assert "playerRef.AddItem(BS01_Valdez_Terminal_Password, 1, True)" in terminal_password

    families = (
        ((910, 911), 912, 913, "CoreProcessingUnit", "CPU"),
        ((920, 921), 922, 923, "IonAccelerator", "IonAccelerator"),
        ((930, 931), 932, 933, "PressureGauge", "PressureGauge"),
        ((1010, 1011), 1012, 1013, "InductionCoil", "InductionCoil"),
    )
    for pristine_stages, damaged_stage, destroyed_stage, item_name, marker_name in families:
        for stage in pristine_stages:
            body = _stage_body(stage)
            assert f"BS01_Invention_{item_name}_MiscItem" in body
            assert f"Alias_{marker_name}_XMarker.GetReference()" in body
        assert f"BS01_Invention_{item_name}_Damaged_MiscItem" in _stage_body(damaged_stage)
        destroyed = _stage_body(destroyed_stage)
        assert f"BS01_Invention_{item_name}_Destroyed_MiscItem" in destroyed
        assert "markerRef.PlaceAtMe(ExtractedExplosion)" in destroyed
        assert "StaggerSpell.Cast(playerRef, playerRef)" in destroyed

    quest_stage_event = _member_body(_patch(QUEST_SCRIPT), "onstageset")
    for choice_stage in (910, 911, 912, 913, 920, 921, 922, 923, 930, 931, 932, 933):
        assert f"IsStageDone({choice_stage})" in quest_stage_event
        assert (
            "BS01_Invention_ObjectivePercentProgress_Global.SetValue("
            "Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)"
            in _stage_body(choice_stage)
        )
    progress_formula = (
        "BS01_Invention_ObjectivePercentProgress_Global.SetValue("
        "Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)"
    )
    assert sum(
        _stage_body(stage).count(progress_formula)
        for stage in (910, 911, 912, 913, 920, 921, 922, 923, 930, 931, 932, 933)
    ) == 12
    assert "BS01_Invention_ObjectivePercentProgress_Global.SetValue(0.0)" in _stage_body(900)
    assert "SetStage(InitialComponentsGatheredStage)" in quest_stage_event
    assert "SetStage(UltraciteBatteryExtractionStage)" in quest_stage_event
    assert "BS01_MQ02_Invention_Valdez_PoorExtraction_Scene.Start()" in quest_stage_event


def test_battery_robot_fight_cleanup_and_story_handoff_are_single_player_safe():
    dirt_swap = _stage_body(1150)
    assert "dirtBagRef.SetValue(BS01_MQ02_Invention_DirtSackSwap_AV, 1.0)" in dirt_swap

    battery = _stage_body(1200)
    assert "BS01_Invention_UltraciteBattery_MiscItem" in battery
    assert "Alias_UltraciteBattery.ForceRefTo(batteryRef)" in battery
    assert 'Game.GetFormFromFile(0x005B70FD, "SeventySix.esm") as ActorBase' in battery
    assert "RefCollectionAlias robotEnemies = GetAlias(31) as RefCollectionAlias" in battery
    assert "While spawnIndex < 2" in battery
    assert "spawnPoint.PlaceActorAtMe(securityRobotBase, 1)" in battery
    assert "robotEnemies.AddRef(enemyRef)" in battery
    assert "enemyRef.StartCombat(playerRef)" in battery
    assert "BS01_Invention_Valdez_UltraciteFightStart_Scene.Start()" in battery

    quest_patch = _patch(QUEST_SCRIPT)
    stage_event = _member_body(quest_patch, "onstageset")
    assert "RefCollectionAlias wiringEnemies = GetAlias(35) as RefCollectionAlias" in stage_event
    assert 'RegisterForRemoteEvent(enemyRef, "OnDeath")' in stage_event
    assert "Alias_UltraciteFight_Enemies_RefCollection.GetCount() == 0" not in stage_event
    death_event = _member_body(quest_patch, "actor.ondeath")
    assert "foundWiringEnemy && allWiringEnemiesDead" in death_event
    assert "SetStage(750)" in death_event
    assert "foundRobotEnemy && allEnemiesDead" in death_event
    assert "allEnemiesDead = False" in death_event
    assert "SetStage(EWSEndStage)" in death_event

    location_change = _member_body(_patch(PLAYER_SCRIPT), "onlocationchange")
    assert "myQI.SetStage(CleanupStage)" in location_change
    assert "myQI.IsStageDone(RobotFightStage)" in location_change
    assert "!myQI.IsStageDone(RobotFightEndedStage)" in location_change
    assert "mySpawnCenter.Activate(GetReference())" in location_change

    handoff = _stage_body(9000)
    assert "If TryStartFieldTesting()" in handoff
    assert "StartTimer(5.0, 9000)" in handoff
    assert ".Start()" not in handoff
    assert "Stop()" in _stage_body(10000)

    combined = "\n".join(_patch(script_name) for script_name in EXPECTED_MEMBERS)
    for forbidden in (
        "DefaultQuestEncounterWaveScript",
        "SendCustomEvent",
        ".SendStoryEvent(",
        "Community",
    ):
        assert forbidden not in combined


def test_field_testing_handoff_retries_rejection_and_defers_cleanup():
    qf_patch = _patch(QF_SCRIPT)
    handoff = _stage_body(9000)
    try_start = _member_body(qf_patch, "trystartfieldtesting")
    retry = _member_body(qf_patch, "ontimer")
    cleanup = _stage_body(10000)

    assert "If IsObjectiveCompleted(1600)" in try_start
    assert "Return True" in try_start
    story_event = (
        "BS01_FieldTesting_QuestStartKeyword.SendStoryEventAndWait("
        "None, playerRef, playerRef)"
    )
    assert story_event in try_start
    assert try_start.index(story_event) < try_start.index("SetObjectiveCompleted(1600)")
    assert "Return IsObjectiveCompleted(1600)" in try_start
    assert ".Start()" not in try_start
    assert ".SendStoryEvent(" not in try_start

    for attempt in (handoff, retry, cleanup):
        assert "If TryStartFieldTesting()" in attempt
        assert "StartTimer(5.0, 9000)" in attempt
        assert (
            attempt.index("If TryStartFieldTesting()")
            < attempt.index("Stop()")
            < attempt.index("Else")
            < attempt.index("StartTimer(5.0, 9000)")
        )
        assert ".Start()" not in attempt
        assert ".SendStoryEvent(" not in attempt

    assert "If aiTimerID == 9000 && IsStageDone(9000)" in retry
    assert "!IsStageDone(10000)" not in retry
    assert "If IsStageDone(10000)" in handoff
    assert handoff.index("If IsStageDone(10000)") < handoff.index("Stop()")
    assert "If IsStageDone(10000)" in retry
    assert retry.index("If IsStageDone(10000)") < retry.index("Stop()")
    assert "SetStage(10000)" not in handoff
    assert "SetStage(10000)" not in retry

    assert "Alias_Valdez_Dungeon_Ref.GetReference()" in cleanup
    assert "Alias_AmbientEnemies_EnableMarker.GetReference()" in cleanup
    assert "Alias_AmbientInsectsEWS_EnableMarker.GetReference()" in cleanup
    assert cleanup.count(".Disable()") == 3
    assert cleanup.rindex(".Disable()") < cleanup.index("If TryStartFieldTesting()")
    assert cleanup.index("If TryStartFieldTesting()") < cleanup.index("Stop()")
    assert cleanup.index("Stop()") < cleanup.index("StartTimer(5.0, 9000)")
    assert "Stop()" in cleanup


def test_stage_registration_reconciles_enemies_that_died_during_the_wait():
    stage_event = _member_body(_patch(QUEST_SCRIPT), "onstageset")

    wiring_stage = stage_event.index("ElseIf auiStageID == 740")
    wiring_registration = stage_event.index(
        'RegisterForRemoteEvent(enemyRef, "OnDeath")', wiring_stage
    )
    wiring_reconciliation = stage_event.index(
        "If foundWiringEnemy && allWiringEnemiesDead && !IsStageDone(750)",
        wiring_registration,
    )
    assert "foundWiringEnemy = False" in stage_event[wiring_stage:wiring_reconciliation]
    assert "allWiringEnemiesDead = True" in stage_event[
        wiring_stage:wiring_reconciliation
    ]
    assert "If !enemyRef.IsDead()" in stage_event[wiring_stage:wiring_registration]
    assert "SetStage(750)" in stage_event[wiring_reconciliation:]

    robot_stage = stage_event.index("ElseIf auiStageID == 1200")
    robot_registration = stage_event.index(
        'RegisterForRemoteEvent(enemyRef, "OnDeath")', robot_stage
    )
    robot_reconciliation = stage_event.index(
        "If foundRobotEnemy && allRobotEnemiesDead && !IsStageDone(EWSEndStage)",
        robot_registration,
    )
    assert "If Alias_UltraciteFight_Enemies_RefCollection != None" in stage_event[
        robot_stage:robot_registration
    ]
    assert "foundRobotEnemy = False" in stage_event[robot_stage:robot_reconciliation]
    assert "allRobotEnemiesDead = True" in stage_event[
        robot_stage:robot_reconciliation
    ]
    assert "If !enemyRef.IsDead()" in stage_event[robot_stage:robot_registration]
    assert "SetStage(EWSEndStage)" in stage_event[robot_reconciliation:]

    assert "wiringEnemies.GetCount() == 0" not in stage_event
    assert "Alias_UltraciteFight_Enemies_RefCollection.GetCount() == 0" not in stage_event


def test_spawned_ews_actors_are_released_and_deleted_after_combat():
    stage_event = _member_body(_patch(QUEST_SCRIPT), "onstageset")

    wiring_cleanup_start = stage_event.index("ElseIf auiStageID == 750")
    robot_cleanup_start = stage_event.index("ElseIf auiStageID == EWSEndStage")
    wiring_cleanup = stage_event[wiring_cleanup_start:robot_cleanup_start]
    robot_cleanup = stage_event[robot_cleanup_start:]

    assert "Int enemyIndex = wiringEnemies.GetCount() - 1" in wiring_cleanup
    assert "enemyIndex -= 1" in wiring_cleanup
    assert (
        wiring_cleanup.index('UnregisterForRemoteEvent(enemyRef, "OnDeath")')
        < wiring_cleanup.index("wiringEnemies.RemoveRef(enemyRef)")
        < wiring_cleanup.index("enemyRef.Disable()")
        < wiring_cleanup.index("enemyRef.Delete()")
    )

    assert "If Alias_UltraciteFight_Enemies_RefCollection != None" in robot_cleanup
    assert (
        "Int enemyIndex = Alias_UltraciteFight_Enemies_RefCollection.GetCount() - 1"
        in robot_cleanup
    )
    assert "enemyIndex -= 1" in robot_cleanup
    assert (
        robot_cleanup.index('UnregisterForRemoteEvent(enemyRef, "OnDeath")')
        < robot_cleanup.index(
            "Alias_UltraciteFight_Enemies_RefCollection.RemoveRef(enemyRef)"
        )
        < robot_cleanup.index("enemyRef.Disable()")
        < robot_cleanup.index("enemyRef.Delete()")
    )


def test_full_production_merges_native_compile_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    for script_name in EXPECTED_MEMBERS:
        result = compile_psc(
            _merged(script_name),
            imports=[str(base_source), str(SOURCE_ROOT)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=f"{script_name.replace(':', '/')}.psc",
        )

        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}: {diagnostics}"
        assert result.pex_bytes is not None
