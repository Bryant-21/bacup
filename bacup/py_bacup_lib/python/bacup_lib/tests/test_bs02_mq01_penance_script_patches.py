from __future__ import annotations

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
SCRIPT_NAME = "Fragments:Quests:QF_BS02_MQ01_Penance_005F38F1"
QUEST_SCRIPT_NAME = "Quests:BS02_MQ01_Penance:QuestScript"
MINE_SCENE_SCRIPT_NAME = "Fragments:Scenes:SF_BS02_MQ01_Penance_MineS_005F72AF_1"
ON_CONNECT_SCRIPT_NAME = (
    "Fragments:Quests:QF_BS02_MQ01_Penance_OnConne_00606C1C"
)
PATCH_MEMBERS = {
    "fragment_stage_0001_item_00",
    "fragment_stage_0002_item_00",
    "fragment_stage_0003_item_00",
    "fragment_stage_0004_item_00",
    "fragment_stage_0010_item_00",
    "fragment_stage_0020_item_00",
    "fragment_stage_0030_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0150_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0210_item_00",
    "fragment_stage_0220_item_00",
    "fragment_stage_0225_item_00",
    "fragment_stage_0227_item_00",
    "fragment_stage_0228_item_00",
    "fragment_stage_0230_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0310_item_00",
    "fragment_stage_0320_item_00",
    "fragment_stage_0326_item_00",
    "fragment_stage_0327_item_00",
    "fragment_stage_0330_item_00",
    "fragment_stage_0340_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0450_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_0750_item_00",
    "fragment_stage_0770_item_00",
    "fragment_stage_0780_item_00",
    "fragment_stage_0800_item_00",
    "fragment_stage_0850_item_00",
    "fragment_stage_0900_item_00",
    "fragment_stage_0995_item_00",
    "fragment_stage_1000_item_00",
    "fragment_stage_1005_item_00",
    "fragment_stage_1010_item_00",
    "fragment_stage_1200_item_00",
    "fragment_stage_1250_item_00",
    "fragment_stage_1260_item_00",
    "fragment_stage_1270_item_00",
    "fragment_stage_1280_item_00",
    "fragment_stage_1304_item_00",
    "fragment_stage_1305_item_00",
    "fragment_stage_1310_item_00",
    "fragment_stage_1315_item_00",
    "fragment_stage_1320_item_00",
    "fragment_stage_1325_item_00",
    "fragment_stage_1327_item_00",
    "fragment_stage_1328_item_00",
    "fragment_stage_1330_item_00",
    "fragment_stage_1340_item_00",
    "fragment_stage_1350_item_00",
    "fragment_stage_1400_item_00",
    "fragment_stage_1500_item_00",
    "fragment_stage_1600_item_00",
    "fragment_stage_1650_item_00",
    "fragment_stage_1700_item_00",
    "fragment_stage_1800_item_00",
    "fragment_stage_1850_item_00",
    "fragment_stage_1900_item_00",
    "fragment_stage_2000_item_00",
    "fragment_stage_2100_item_00",
    "fragment_stage_2200_item_00",
    "fragment_stage_2300_item_00",
    "fragment_stage_2350_item_00",
    "fragment_stage_2360_item_00",
    "fragment_stage_2400_item_00",
    "fragment_stage_2450_item_00",
    "fragment_stage_2500_item_00",
    "fragment_stage_2600_item_00",
    "fragment_stage_9000_item_00",
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
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source_for(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


def _merged_source() -> str:
    return _merged_source_for(SCRIPT_NAME)


def _on_connect_merged_source() -> str:
    patch = _script_patch_source(ON_CONNECT_SCRIPT_NAME)
    assert patch is not None
    skeleton = (
        "Scriptname "
        "Fragments:Quests:QF_BS02_MQ01_Penance_OnConne_00606C1C "
        "Extends Quest hidden\n\n"
        "ReferenceAlias Property Alias_Player Auto mandatory\n"
        "Quest Property BS02_MQ01_Penance Auto\n"
        "Keyword Property BS02_MQ01_Penance_StartKeyword Auto\n\n"
        "Function Fragment_Stage_0100_Item_00()\n"
        "EndFunction\n"
    )
    return _merge_script_method_patches(skeleton, patch)


def test_penance_patch_implements_every_vmad_member_once() -> None:
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert len(PATCH_MEMBERS) == 73
    assert set(_member_names(patch)) == PATCH_MEMBERS
    assert len(_member_names(patch)) == len(PATCH_MEMBERS)
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )
    for member in PATCH_MEMBERS:
        assert len(_member_body(patch, member).splitlines()) > 2


def test_penance_production_merge_is_unique_idempotent_and_compiles(
    tmp_path: Path,
) -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merged_source()
    merged_members = _member_names(merged)

    for member in PATCH_MEMBERS:
        assert merged_members.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    compiled_sources = {
        SCRIPT_NAME: _merged_source_for(SCRIPT_NAME),
        QUEST_SCRIPT_NAME: _merged_source_for(QUEST_SCRIPT_NAME),
        MINE_SCENE_SCRIPT_NAME: _merged_source_for(MINE_SCENE_SCRIPT_NAME),
        ON_CONNECT_SCRIPT_NAME: _on_connect_merged_source(),
    }
    for compiled_script_name, source in compiled_sources.items():
        relative_path = _script_relative_path(compiled_script_name, ".psc")
        merged_path = tmp_path / relative_path
        merged_path.parent.mkdir(parents=True, exist_ok=True)
        merged_path.write_text(source, encoding="utf-8")

    for compiled_script_name, source in compiled_sources.items():
        result = compile_psc(
            source,
            imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(compiled_script_name, ".psc")),
        )

        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, diagnostics
        assert result.pex_bytes is not None


def test_search_report_and_tool_routes_converge_once() -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    convergence = (
        ("fragment_stage_1000_item_00", 1010, 1200),
        ("fragment_stage_1010_item_00", 1000, 1200),
        ("fragment_stage_1250_item_00", 1260, 1270),
        ("fragment_stage_1260_item_00", 1250, 1270),
        ("fragment_stage_1305_item_00", 1328, 1350),
        ("fragment_stage_1328_item_00", 1305, 1350),
    )
    for member, sibling_stage, destination_stage in convergence:
        body = _member_body(patch, member)
        assert f"IsStageDone({sibling_stage})" in body
        assert f"!IsStageDone({destination_stage})" in body
        assert body.count(f"SetStage({destination_stage})") == 1

    for member in (
        "fragment_stage_1310_item_00",
        "fragment_stage_1320_item_00",
        "fragment_stage_1330_item_00",
    ):
        body = _member_body(patch, member)
        assert "!IsStageDone(1400)" in body
        assert body.count("SetStage(1400)") == 1

    tools = _member_body(patch, "fragment_stage_1270_item_00")
    assert "Alias_QO_Weap_Shovel.ForceRefTo(shovelRef)" in tools
    assert "Alias_QO_Weap_Dynamite.ForceRefTo(dynamiteRef)" in tools
    assert "BS02_MQ01_Penance_InitiatesCrawlThroughCrevice.Start()" in tools


def test_early_cavern_objectives_preserve_both_follow_and_report_cycles() -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    expected_lifecycle = {
        "fragment_stage_0700_item_00": (60, 65),
        "fragment_stage_0770_item_00": (65, 68),
        "fragment_stage_0780_item_00": (68, 60),
        "fragment_stage_0850_item_00": (60, 65),
        "fragment_stage_0900_item_00": (65, 70),
    }
    for member, (completed, displayed) in expected_lifecycle.items():
        body = _member_body(patch, member)
        assert f"SetObjectiveCompleted({completed})" in body
        assert f"SetObjectiveDisplayed({displayed})" in body


def test_rockslide_and_both_combat_waves_use_local_bound_aliases() -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    rockslide = _member_body(patch, "fragment_stage_1400_item_00")
    assert "Alias_Activator_Rockslide.GetReference()" in rockslide
    assert "Alias_Static_RockNavBlocker.GetReference()" in rockslide
    assert "rockslide.PlaceAtMe(ExplosionVault79EntranceRocks)" in rockslide
    assert "Alias_EnableMarker_Caverns.GetReference()" in rockslide
    assert "Alias_SpawnCenter_Wave1.GetReference()" in rockslide
    assert rockslide.count("spawnCenter.Enable()") == 1
    assert "questScript.StartLocalWave(spawnCenter, 1500)" in rockslide

    blast = _member_body(patch, "fragment_stage_2200_item_00")
    assert "Alias_Static_Mine.GetReference()" in blast
    assert "mine.PlaceAtMe(ExplosionFragMine)" in blast
    assert "Alias_SpawnCenter_Int.GetReference()" in blast
    assert blast.count("spawnCenter.Enable()") == 1
    assert "questScript.ResolveMineBlast()" in blast
    assert blast.index("questScript.ResolveMineBlast()") < blast.index(
        "mine.PlaceAtMe(ExplosionFragMine)"
    )
    assert "questScript.StartLocalWave(spawnCenter, 2300)" in blast
    assert "shin.AddToFaction(CaptiveFaction)" in blast
    assert "shin.ChangeAnimFaceArchetype(AnimFaceArchetypeInPain)" in blast

    assert "EWS" not in patch
    assert "Session" not in patch
    assert "Game.GetPlayer" not in patch


def test_local_wave_helper_spawns_tracks_and_never_empty_autoclears() -> None:
    patch = _script_patch_source(QUEST_SCRIPT_NAME)
    assert patch is not None

    start = _member_body(patch, "startlocalwave")
    assert "GetAlias(28) as RefCollectionAlias" in start
    assert (
        'Game.GetFormFromFile(0x00605C2D, "SeventySix.esm") as ActorBase'
        in start
    )
    assert "GetRequestedLocalWaveCount(waveConfig)" in start
    assert "While enemyIndex < totalEnemyCount" in start
    assert (
        "existingEnemy.IsCreated() && existingEnemy.GetActorBase() == enemyBase"
        in start
    )
    assert "localEnemies.RemoveAll()" not in start
    assert "CleanupLocalWaveRefs(localEnemies, enemyBase)" in start
    assert "spawnCenter.PlaceActorAtMe(enemyBase, levelModifier)" in start
    assert start.index("If enemyRef != None") < start.index(
        "localEnemies.AddRef(enemyRef)"
    )
    assert 'RegisterForRemoteEvent(enemyRef, "OnDeath")' in start
    assert "enemyRef.StartCombat(player, True)" in start
    assert "CompleteLocalWaveIfAllDead()" not in start
    assert "SetStage(" not in start

    completion = _member_body(patch, "completelocalwaveifalldead")
    assert "If enemyCount <= 0" in completion
    assert completion.index("If enemyCount <= 0") < completion.index(
        "SetStage(2300)"
    )
    assert "If !foundEnemy" in completion
    assert "If !enemyRef.IsDead()" in completion
    assert (
        "enemyRef.IsCreated() && enemyRef.GetActorBase() == enemyBase"
        in completion
    )
    assert "GetAlias(27) as ReferenceAlias" in completion
    assert "GetAlias(37) as ReferenceAlias" in completion
    assert "expectedEnemyCount = GetRequestedLocalWaveCount(waveConfig)" in completion
    assert "localEnemyCount += 1" in completion
    assert "If localEnemyCount != expectedEnemyCount" in completion
    assert "CleanupLocalWaveRefs(localEnemies, enemyBase)" in completion
    assert "localEnemies.RemoveAll()" not in completion
    assert completion.index("If !foundEnemy") < completion.index(
        "If localEnemyCount != expectedEnemyCount"
    )
    assert completion.index("If localEnemyCount != expectedEnemyCount") < (
        completion.index("CleanupLocalWaveRefs(localEnemies, enemyBase)")
    )
    assert completion.index("CleanupLocalWaveRefs(localEnemies, enemyBase)") < (
        completion.index("SetStage(2300)")
    )
    assert completion.count("SetStage(1500)") == 1
    assert completion.count("SetStage(2300)") == 1

    cleanup = _member_body(patch, "cleanuplocalwaverefs")
    assert "localEnemies.GetCount() - 1" in cleanup
    assert "While enemyIndex >= 0" in cleanup
    assert "enemyIndex -= 1" in cleanup
    assert "enemyRef.IsCreated() && enemyRef.GetActorBase() == enemyBase" in cleanup
    assert "localEnemies.RemoveAll()" not in cleanup
    cleanup_calls = (
        'UnregisterForRemoteEvent(enemyRef, "OnDeath")',
        "localEnemies.RemoveRef(enemyRef)",
        "enemyRef.Disable()",
        "enemyRef.Delete()",
    )
    assert [cleanup.index(call) for call in cleanup_calls] == sorted(
        cleanup.index(call) for call in cleanup_calls
    )

    death = _member_body(patch, "actor.ondeath")
    assert "localEnemies.Find(akSender) >= 0" in death
    assert death.count("CompleteLocalWaveIfAllDead()") == 1


def test_local_wave_population_uses_finite_repeats_and_additive_final_boss() -> None:
    patch = _script_patch_source(QUEST_SCRIPT_NAME)
    assert patch is not None

    start = _member_body(patch, "startlocalwave")
    requested_count = _member_body(patch, "getrequestedlocalwavecount")
    tunnel_regular_count = 2 * 1
    interior_regular_count = 2 * 3

    assert tunnel_regular_count == 2
    assert interior_regular_count == 6
    assert interior_regular_count + 1 == 7
    assert "regularEnemiesPerRepeat = waveConfig.ActorSkew" in requested_count
    assert (
        "If waveConfig.RepeatType == 2 && waveConfig.RepeatNumber > 0"
        in requested_count
    )
    assert "repeatCount = waveConfig.RepeatNumber" in requested_count
    assert (
        "requestedEnemyCount = regularEnemiesPerRepeat * repeatCount"
        in requested_count
    )
    assert "requestedEnemyCount += 1" in requested_count
    assert "Return requestedEnemyCount" in requested_count
    assert "regularEnemyCount = totalEnemyCount" in start
    assert "regularEnemyCount -= 1" in start
    assert start.index("While enemyIndex < totalEnemyCount") < start.index(
        "enemyIndex == regularEnemyCount"
    )
    assert start.index("enemyIndex == regularEnemyCount") < start.index(
        "levelModifier = 3"
    )
    assert start.index("levelModifier = 3") < start.index(
        "spawnCenter.PlaceActorAtMe(enemyBase, levelModifier)"
    )


def test_local_wave_requires_player_and_rolls_back_partial_population() -> None:
    patch = _script_patch_source(QUEST_SCRIPT_NAME)
    assert patch is not None

    start = _member_body(patch, "startlocalwave")
    spawn = "spawnCenter.PlaceActorAtMe(enemyBase, levelModifier)"
    partial_guard = "If spawnedEnemyCount != totalEnemyCount"

    assert "Actor player = Alias_Player.GetActorReference()" in start
    assert "player == None" in start
    assert start.index("player == None") < start.index(spawn)
    assert "Int spawnedEnemyCount = 0" in start
    assert "localEnemies.Find(enemyRef) >= 0" in start
    assert start.index("localEnemies.Find(enemyRef) >= 0") < start.index(
        "spawnedEnemyCount += 1"
    )
    assert "Bool spawnCommitted = False" in start
    assert "If !spawnCommitted" in start
    assert partial_guard in start
    rollback_start = start.index(partial_guard)
    commit_start = start.index('RegisterForRemoteEvent(enemyRef, "OnDeath")')
    assert rollback_start < commit_start
    rollback = start[rollback_start:commit_start]
    assert "CleanupLocalWaveRefs(localEnemies, enemyBase)" in rollback
    assert "Return" in rollback
    assert "enemyRef.StartCombat(player, True)" not in rollback
    assert start.index("enemyRef.Disable()") < start.index("enemyRef.Delete()")
    assert start.index('RegisterForRemoteEvent(enemyRef, "OnDeath")') < (
        start.index("enemyRef.StartCombat(player, True)")
    )


def test_mine_scene_waits_then_teleports_and_uses_knockdown_idle() -> None:
    quest_fragment_patch = _script_patch_source(SCRIPT_NAME)
    scene_patch = _script_patch_source(MINE_SCENE_SCRIPT_NAME)
    quest_patch = _script_patch_source(QUEST_SCRIPT_NAME)
    assert quest_fragment_patch is not None
    assert scene_patch is not None
    assert quest_patch is not None

    phase = _member_body(scene_patch, "fragment_phase_02_begin")
    assert (
        'Game.GetFormFromFile(0x005F38F1, "SeventySix.esm") as Quest'
        in phase
    )
    assert phase.count("questScript.PrepareMineScene()") == 1
    assert "SetStage(" not in phase

    prepare = _member_body(quest_patch, "prepareminescene")
    assert "ShinReachedMineSceneMarker = False" in prepare
    assert "Alias_Actor_Shin_CavernsInt.GetActorReference()" in prepare
    assert "Alias_Marker_ShinMineScene.GetReference()" in prepare
    assert "waited < ShinTeleportFallbackTime" in prepare
    assert "Utility.Wait(waitInterval)" in prepare
    assert "shin.MoveTo(mineSceneMarker)" in prepare
    assert "shin.PlayIdle(IdleMineExplosionReadyLoop)" in prepare
    assert prepare.index("shin.MoveTo(mineSceneMarker)") < prepare.index(
        "ShinReachedMineSceneMarker = True"
    )
    assert "SetStage(" not in prepare

    ready = _member_body(quest_fragment_patch, "fragment_stage_2100_item_00")
    assert "shin.PlayIdle(IdleMineExplosionReady)" in ready

    blast = _member_body(quest_patch, "resolvemineblast")
    assert "shin.PlayIdle(IdleMineExplosionReady_Knockdown)" in blast
    assert "SetStage(" not in blast


def test_penance_helper_patches_merge_once_and_are_idempotent() -> None:
    expected_members = {
        QUEST_SCRIPT_NAME: {
            "startlocalwave",
            "completelocalwaveifalldead",
            "getrequestedlocalwavecount",
            "cleanuplocalwaverefs",
            "attemptmissinghandoff",
            "ontimer",
            "prepareminescene",
            "resolvemineblast",
            "actor.ondeath",
        },
        MINE_SCENE_SCRIPT_NAME: {"fragment_phase_02_begin"},
    }

    for script_name, members in expected_members.items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert set(_member_names(patch)) == members
        merged = _merged_source_for(script_name)
        merged_members = _member_names(merged)
        for member in members:
            assert merged_members.count(member) == 1
            assert _member_body(merged, member) == _member_body(patch, member)
        assert _merge_script_method_patches(merged, patch) == merged


def test_pip_boy_progresses_to_acknowledged_retrying_missing_handoff() -> None:
    quest_fragment_patch = _script_patch_source(SCRIPT_NAME)
    quest_patch = _script_patch_source(QUEST_SCRIPT_NAME)
    assert quest_fragment_patch is not None
    assert quest_patch is not None

    approach = _member_body(quest_fragment_patch, "fragment_stage_2450_item_00")
    assert "Alias_Marker_PipBoy.GetReference()" in approach
    assert "pipBoyMarker.PlaceAtMe(BS02_MQ01_Penance_PipBoy)" in approach
    assert "Alias_QO_Misc_PipBoy.ForceRefTo(pipBoyRef)" in approach

    pickup = _member_body(quest_fragment_patch, "fragment_stage_2500_item_00")
    assert pickup.index("SetObjectiveCompleted(230)") < pickup.index(
        "SetObjectiveDisplayed(240)"
    )

    shin = _member_body(quest_fragment_patch, "fragment_stage_2600_item_00")
    assert shin.index("SetObjectiveCompleted(240)") < shin.index(
        "SetObjectiveDisplayed(250)"
    )

    terminal = _member_body(quest_fragment_patch, "fragment_stage_9000_item_00")
    send = (
        "accepted = startKeyword.SendStoryEventAndWait(None, player, player)"
    )
    assert "pipBoyRef.GetContainer() == playerRef" in terminal
    assert "playerRef.RemoveItem(pipBoyRef, 1, True)" in terminal
    assert (
        "questScript.AttemptMissingHandoff(BS02_MQ02_Missing, "
        "BS02_MQ02_Missing_StartKeyword)"
        in terminal
    )
    assert "SendStoryEventAndWait" not in terminal
    assert "Stop()" not in terminal

    attempt = _member_body(quest_patch, "attemptmissinghandoff")
    state_ack = (
        "accepted = missingQuest.IsRunning() || missingQuest.IsCompleted()"
    )
    assert "Bool accepted = False" in attempt
    assert attempt.count(state_ack) == 2
    assert attempt.index(state_ack) < attempt.index(send)
    assert attempt.rindex(state_ack) > attempt.index(send)
    assert attempt.count(send) == 1
    assert (
        "If accepted\n        Stop()\n    Else\n        StartTimer(5.0, 9000)"
        in attempt
    )
    assert attempt.count("Stop()") == 1
    assert attempt.count("StartTimer(5.0, 9000)") == 1
    assert ".Start()" not in attempt

    timer = _member_body(quest_patch, "ontimer")
    assert "If aiTimerID != 9000 || !IsStageDone(9000)" in timer
    assert (
        'Game.GetFormFromFile(0x005F56C4, "SeventySix.esm") as Quest'
        in timer
    )
    assert (
        'Game.GetFormFromFile(0x005F56B8, "SeventySix.esm") as Keyword'
        in timer
    )
    assert "AttemptMissingHandoff(missingQuest, startKeyword)" in timer
    assert "SendStoryEventAndWait" not in timer
    assert "Stop()" not in timer


def test_penance_on_connect_retries_until_bound_quest_acknowledges() -> None:
    patch = _script_patch_source(ON_CONNECT_SCRIPT_NAME)
    assert patch is not None
    expected_members = {
        "fragment_stage_0100_item_00",
        "attemptpenancehandoff",
        "ontimer",
    }
    assert set(_member_names(patch)) == expected_members
    merged = _on_connect_merged_source()
    for member in expected_members:
        assert _member_names(merged).count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged

    stage = _member_body(patch, "fragment_stage_0100_item_00")
    assert stage.count("AttemptPenanceHandoff()") == 1
    assert "SendStoryEventAndWait" not in stage
    assert "Stop()" not in stage

    attempt = _member_body(patch, "attemptpenancehandoff")
    state_ack = (
        "accepted = BS02_MQ01_Penance.IsRunning() || "
        "BS02_MQ01_Penance.IsCompleted()"
    )
    send = (
        "accepted = BS02_MQ01_Penance_StartKeyword."
        "SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    assert "Bool accepted = False" in attempt
    assert attempt.count(state_ack) == 2
    assert attempt.index(state_ack) < attempt.index(send)
    assert attempt.rindex(state_ack) > attempt.index(send)
    assert attempt.count(send) == 1
    assert (
        "If accepted\n        Stop()\n    Else\n        StartTimer(5.0, 100)"
        in attempt
    )
    assert attempt.count("Stop()") == 1
    assert ".Start()" not in attempt

    timer = _member_body(patch, "ontimer")
    assert "If aiTimerID != 100 || !IsStageDone(100)" in timer
    assert timer.count("AttemptPenanceHandoff()") == 1
    assert "SendStoryEventAndWait" not in timer
    assert "Stop()" not in timer
