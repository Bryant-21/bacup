from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
FRAGMENT_SCRIPT = "Fragments:Quests:QF_BS01_MQ08_Defense_005D951E"
QUEST_SCRIPT = "Quests:BS01_MQ08_Defense:QuestScript"
BOUND_FRAGMENT_MEMBERS = (
    "fragment_stage_0520_item_00",
    "fragment_stage_0250_item_00",
    "fragment_stage_0415_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_0540_item_00",
    "fragment_stage_0002_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_0480_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0510_item_00",
    "fragment_stage_0001_item_00",
    "fragment_stage_0350_item_00",
    "fragment_stage_0800_item_00",
    "fragment_stage_9000_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0003_item_00",
    "fragment_stage_0475_item_00",
    "fragment_stage_0850_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0825_item_00",
    "fragment_stage_0410_item_00",
    "fragment_stage_0150_item_00",
    "fragment_stage_0450_item_00",
    "fragment_stage_0420_item_00",
    "fragment_stage_0425_item_00",
    "fragment_stage_0530_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0830_item_00",
)


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
        for kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if kind in {"function", "event"} and name == member_name.casefold()
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


def _assert_in_order(source: str, *tokens: str) -> None:
    positions = [source.index(token) for token in tokens]
    assert positions == sorted(positions)


def test_fragment_patch_covers_the_exact_live_vmad_contract_once() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    fragment_members = [
        member
        for member in _member_names(patch)
        if member.startswith("fragment_stage_")
    ]

    assert tuple(fragment_members) == BOUND_FRAGMENT_MEMBERS
    assert Counter(fragment_members) == Counter(BOUND_FRAGMENT_MEMBERS)
    assert len(fragment_members) == 29
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} "
        for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", (FRAGMENT_SCRIPT, QUEST_SCRIPT))
def test_production_merges_are_unique_and_idempotent(script_name: str) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_source(script_name)
    merged_members = _member_names(merged)

    for member in _member_names(patch):
        assert merged_members.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert merged.casefold().count("scriptname ") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_quest_controller_uses_only_top_level_members_and_standard_events() -> None:
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None
    members = _member_names(patch)

    assert Counter(members) == Counter(set(members))
    assert not _iter_papyrus_states(patch.splitlines())
    assert "onquestinit" in members
    assert "onstageset" in members
    assert "ontimer" in members
    assert "actor.ondeath" in members
    assert "actor.onplayerloadgame" in members
    assert "onquestshutdown" in members
    assert "Event OnStageSet(Int auiStageID, Int auiItemID)" in patch
    assert "Event Actor.OnDeath(Actor akSender, Actor akKiller)" in patch
    assert "Event Actor.OnPlayerLoadGame(Actor akSender)" in patch
    assert not any(
        " property " in f" {line.strip().casefold()} "
        for line in patch.splitlines()
    )


def test_prior_choices_feed_two_persisted_bonus_conversation_slots() -> None:
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None
    build = _member_body(patch, "buildavailableconversations")
    add = _member_body(patch, "addconversation")
    play = _member_body(patch, "playnextconversation")
    record = _member_body(patch, "recordconversationchoice")

    for actor_value in (
        "BS01_MQ03_FieldTesting_RecruitedColinPutnam_AV",
        "BS01_MQ03_FieldTesting_RecruitedMartyPutnam_AV",
        "BS01_MQ04_Arms_WeaponChoice",
        "BS01_MQ05_Raiders_AV_Negotiation",
        "BS01_MQ06_Settlers_Decision",
    ):
        assert actor_value in build
    for stage_property in (
        "BonusConvBiAGaveWeaponsStage",
        "BonusConvFieldTestingStage",
        "BonusConvPRBackedDownStage",
        "BonusConvPRStrongArmedRaiderStage",
        "BonusConvSDFoundationKeptWeaponsStage",
        "BonusConvSDGuardFoundationStage",
    ):
        assert stage_property in record
    assert add.count("AvailableConversations.Add(sceneToAdd)") == 1
    assert "ConversationSceneSeed % AvailableConversations.Length" in play
    assert play.count("ConversationSceneSeed += 1") == 1
    assert "RecordConversationChoice(chosenScene)" in play
    assert "StartSceneIfIdle(chosenScene)" in play


def test_all_four_bomb_activations_are_idempotent_and_gate_retreat() -> None:
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None
    stage_event = _member_body(patch, "onstageset")
    placed = _member_body(patch, "setbombplaced")
    sync = _member_body(patch, "syncbombstatics")

    bomb_contracts = {
        510: "Alias_Static_BombA",
        520: "Alias_Static_BombB",
        530: "Alias_Static_BombC",
        540: "Alias_Static_BombD",
    }
    for stage, bomb_alias in bomb_contracts.items():
        assert f"auiStageID == {stage}" in stage_event
        assert f"SetBombPlaced({bomb_alias})" in stage_event
        assert f"IsStageDone({stage})" in placed
        assert f"IsStageDone({stage})" in sync
    assert placed.count("RemoveItem(BS01_MQ08_Defense_Bomb_MiscItem, 1, True)") == 1
    _assert_in_order(
        placed,
        "IsStageDone(510)",
        "IsStageDone(520)",
        "IsStageDone(530)",
        "IsStageDone(540)",
        "SetStage(AllBombsPlantedStage)",
    )
    assert "Bool showBombs = !IsStageDone(PostBombsStage)" in sync


def test_barricade_and_detonation_timers_survive_save_load_without_reentry() -> None:
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None
    timer = _member_body(patch, "ontimer")
    detonate = _member_body(patch, "detonatebombs")
    reconcile = _member_body(patch, "reconcileruntime")
    load_event = _member_body(patch, "actor.onplayerloadgame")

    assert "aiTimerID == BrotherhoodNPCSpawnTimerID" in timer
    assert "!IsStageDone(DungeonNPCsAtBarricadeStage)" in timer
    assert "FinishBrotherhoodBarricadeTravel()" in timer
    assert "aiTimerID == BombDetonationTimerID" in timer
    assert "!IsStageDone(PostBombsStage)" in timer
    assert "DetonateBombs()" in timer
    _assert_in_order(
        detonate,
        "If IsStageDone(PostBombsStage)",
        "Alias_Markers_ExplosionMarkers.GetAt(index)",
        "SetCollectionEnabled(Alias_MovableStatics_VentCollapses, True)",
        "CameraShakeSpell.Cast(playerRef, playerRef)",
        "SetStage(PostBombsStage)",
    )
    assert "IsStageDone(250) && !IsStageDone(DungeonNPCsAtBarricadeStage)" in reconcile
    assert "IsStageDone(700) && !IsStageDone(PostBombsStage)" in reconcile
    assert "akSender == Game.GetPlayer()" in load_event
    assert "ReconcileRuntime()" in load_event


def test_four_local_defense_waves_use_the_live_alias_and_wave_contract() -> None:
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None
    start = _member_body(patch, "startalllocaldefensewaves")
    reconcile = _member_body(patch, "reconcilelocaldefensewave")

    assert "!IsStageDone(500) || IsStageDone(PostBombsStage)" in start
    for wave_name, spawn_alias, collection_alias in (
        ("A", 42, 43),
        ("B", 44, 47),
        ("C", 45, 48),
        ("D", 46, 49),
    ):
        assert (
            f"Bool wave{wave_name}Ready = "
            f"ReconcileLocalDefenseWave({spawn_alias}, {collection_alias})"
            in start
        )
    assert (
        "If !waveAReady || !waveBReady || !waveCReady || !waveDReady"
        in start
    )
    _assert_in_order(start, "CleanupAllLocalDefenseWaves()", "StartTimer(5.0, 30)")

    for actor_form in (
        'Game.GetFormFromFile(0x00075335, "Fallout4.esm") as ActorBase',
        'Game.GetFormFromFile(0x00088F14, "Fallout4.esm") as ActorBase',
        'Game.GetFormFromFile(0x000948B3, "Fallout4.esm") as ActorBase',
    ):
        assert actor_form in reconcile
    assert "0x00075341" not in reconcile
    assert "0x00117D85" not in reconcile
    assert "While enemyIndex < 3" in reconcile
    assert "enemyBaseToSpawn = rangedBase" in reconcile
    assert "enemyBaseToSpawn = meleeBase" in reconcile
    assert "enemyBaseToSpawn = houndBase" in reconcile
    assert reconcile.count(
        "spawnCenter.PlaceActorAtMe(enemyBaseToSpawn, 0)"
    ) == 1
    assert "spawnCenter.PlaceActorAtMe(enemyBaseToSpawn, 2)" not in reconcile
    assert (
        "spawnCenter == None || localEnemies == None || playerRef == None"
        in reconcile
    )
    assert "localEnemies.AddRef(spawnedEnemyRef)" in reconcile
    assert "localEnemies.Find(spawnedEnemyRef) >= 0" in reconcile
    assert "CleanupLocalDefenseWave(localEnemies" in reconcile
    spawn_loop = reconcile.index("While enemyIndex < 3")
    committed_population = reconcile.index(
        'RegisterForRemoteEvent(committedEnemyRef, "OnDeath")'
    )
    committed_combat = reconcile.index(
        "committedEnemyRef.StartCombat(playerRef, True)"
    )
    assert spawn_loop < committed_population < committed_combat
    assert reconcile.rindex("Return True") > committed_combat

    unregister_existing = reconcile.index(
        'UnregisterForRemoteEvent(reconciledEnemyRef, "OnDeath")'
    )
    register_existing = reconcile.index(
        'RegisterForRemoteEvent(reconciledEnemyRef, "OnDeath")'
    )
    assert unregister_existing < register_existing


def test_local_wave_deaths_converge_without_empty_auto_completion() -> None:
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None
    death = _member_body(patch, "actor.ondeath")
    complete = _member_body(patch, "completelocaldefensewaveifalldead")
    cleanup = _member_body(patch, "cleanuplocaldefensewave")

    for collection_alias in (43, 47, 48, 49):
        assert f"GetAlias({collection_alias}) as RefCollectionAlias" in death
    assert death.count("CompleteLocalDefenseWaveIfAllDead(localEnemies)") == 4
    assert "localEnemies.GetCount() <= 0" in complete
    assert "If !scannedEnemyRef.IsDead()" in complete
    assert "rangedCount != 1 || meleeCount != 1 || houndCount != 1" in complete
    assert 'UnregisterForRemoteEvent(completedEnemyRef, "OnDeath")' in complete
    assert "SetStage(" not in complete
    assert "CleanupLocalDefenseWave(" not in complete
    assert "RemoveRef(" not in complete
    _assert_in_order(
        cleanup,
        'UnregisterForRemoteEvent(enemyRef, "OnDeath")',
        "localEnemies.RemoveRef(enemyRef)",
        "enemyRef.Disable()",
        "enemyRef.Delete()",
    )


def test_local_waves_reconcile_on_stage_load_retry_and_terminal_cleanup() -> None:
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None
    stage = _member_body(patch, "onstageset")
    timer = _member_body(patch, "ontimer")
    reconcile = _member_body(patch, "reconcileruntime")
    shutdown = _member_body(patch, "onquestshutdown")

    assert "auiStageID == 500" in stage
    assert "StartAllLocalDefenseWaves()" in stage
    assert "auiStageID == PostBombsStage" in stage
    assert "CancelTimer(30)" in stage
    assert "CleanupAllLocalDefenseWaves()" in stage
    assert "aiTimerID == 30 && IsStageDone(500)" in timer
    assert "!IsStageDone(PostBombsStage)" in timer
    assert "StartAllLocalDefenseWaves()" in timer
    assert "IsStageDone(500) && !IsStageDone(PostBombsStage)" in reconcile
    assert "StartAllLocalDefenseWaves()" in reconcile
    assert "CancelTimer(30)" in shutdown
    assert "CleanupAllLocalDefenseWaves()" in shutdown


def test_fragment_objectives_actor_visibility_and_story_handoff_are_complete() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None

    objective_contracts = {
        100: "SetObjectiveDisplayed(10)",
        200: "SetObjectiveDisplayed(20)",
        300: "SetObjectiveDisplayed(30)",
        400: "SetObjectiveDisplayed(40)",
        500: "SetObjectiveDisplayed(50)",
        600: "SetObjectiveDisplayed(60)",
        700: "SetObjectiveDisplayed(70)",
        800: "SetObjectiveDisplayed(80)",
        825: "SetObjectiveCompleted(85)",
        830: "SetObjectiveFailed(85)",
    }
    for stage, token in objective_contracts.items():
        assert token in _member_body(
            patch, f"fragment_stage_{stage:04d}_item_00"
        )

    finish = _member_body(patch, "fragment_stage_9000_item_00")
    assert finish.count("AttemptPenanceHandoff()") == 1
    assert "SendStoryEventAndWait" not in finish
    assert "Stop()" not in finish
    assert ".Start()" not in finish
    _assert_in_order(finish, "CompleteQuest()", "AttemptPenanceHandoff()")


def test_penance_handoff_retries_until_the_story_manager_acknowledges() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    attempt = _member_body(patch, "attemptpenancehandoff")
    timer = _member_body(patch, "ontimer")
    shutdown = _member_body(patch, "onquestshutdown")
    state_ack = (
        "accepted = BS02_MQ01_Penance.IsRunning() || "
        "BS02_MQ01_Penance.IsCompleted()"
    )
    story_event = (
        "accepted = BS02_MQ01_Penance_StartKeyword."
        "SendStoryEventAndWait(None, playerRef, playerRef)"
    )

    assert "Bool accepted = False" in attempt
    assert attempt.count(state_ack) == 2
    assert attempt.index(state_ack) < attempt.index(story_event)
    assert attempt.rindex(state_ack) > attempt.index(story_event)
    assert attempt.count(story_event) == 1
    assert "BS02_MQ01_Penance.GetAlias(0) as ReferenceAlias" in attempt
    assert "nextPlayerAlias.ForceRefTo(playerRef)" in attempt
    assert attempt.index("nextPlayerAlias.ForceRefTo(playerRef)") < attempt.index(
        "Stop()"
    )
    assert attempt.count("Stop()") == 1
    assert "CancelTimer(9000)" in attempt
    assert attempt.count("StartTimer(5.0, 9000)") == 1
    assert attempt.index("Stop()") < attempt.index("StartTimer(5.0, 9000)")
    assert ".Start()" not in attempt

    assert "If aiTimerID != 9000 || !IsStageDone(9000)" in timer
    assert timer.count("AttemptPenanceHandoff()") == 1
    assert "SendStoryEventAndWait" not in timer
    assert "Stop()" not in timer
    assert "CancelTimer(9000)" in shutdown


@pytest.mark.parametrize("script_name", (FRAGMENT_SCRIPT, QUEST_SCRIPT))
def test_full_production_merge_native_compiles_for_fo4(script_name: str) -> None:
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
