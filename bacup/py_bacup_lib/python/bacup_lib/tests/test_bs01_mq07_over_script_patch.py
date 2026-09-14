from __future__ import annotations

from collections import Counter
from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Fragments:Quests:QF_BS01_MQ07_Over_005DC4FE"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
EXPECTED_STAGES = {
    1,
    2,
    8,
    10,
    100,
    150,
    200,
    290,
    300,
    400,
    430,
    440,
    450,
    500,
    600,
    1000,
    1050,
    1100,
    1150,
    1200,
    1300,
    1350,
    1360,
    1400,
    1500,
    1600,
    1700,
    1800,
    1850,
    1860,
    1880,
    1900,
    2000,
    2050,
    2100,
    2200,
    2400,
    2450,
    2500,
    2550,
    2600,
    2700,
    2800,
    2805,
    2810,
    2820,
    2825,
    2830,
    2840,
    2850,
    2860,
    2890,
    2900,
    3000,
    3100,
    3200,
    3250,
    3300,
    3350,
    3400,
    4000,
    4100,
    4200,
    4300,
    5000,
    5100,
    5200,
    9000,
}
EXPECTED_MEMBERS = {
    f"fragment_stage_{stage:04d}_item_00" for stage in EXPECTED_STAGES
}


def _stage_members(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind == "function" and name.startswith("fragment_stage_")
    ]


def _members(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member_source(source: str, member_name: str) -> str:
    lines = source.splitlines()
    for _kind, name, start, end in _iter_top_level_papyrus_members(lines):
        if name == member_name.lower():
            return "\n".join(lines[start : end + 1])
    raise AssertionError(f"missing Papyrus member: {member_name}")


def _assert_in_order(source: str, *tokens: str) -> None:
    positions = [source.index(token) for token in tokens]
    assert positions == sorted(positions)


def test_bs01_mq07_patch_merges_all_bound_fragments_once_without_duplicates():
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "Scriptname" not in patch
    assert all(count == 1 for count in Counter(_members(patch)).values())
    assert Counter(_stage_members(patch)) == Counter(EXPECTED_MEMBERS)
    assert len(_stage_members(patch)) == 68

    merged = _merge_script_method_patches(skeleton, patch)
    assert all(count == 1 for count in Counter(_members(merged)).values())
    assert Counter(_stage_members(merged)) == Counter(EXPECTED_MEMBERS)
    assert "referencealias Property Alias_Player Auto mandatory" in merged
    assert "scene Property BS01_MQ07_Over_Sabotage Auto" in merged
    assert _merge_script_method_patches(merged, patch) == merged


def test_bs01_mq07_patch_restores_the_local_interaction_spine():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "Alias_Player.ForceRefTo(playerRef)" in patch
    assert "playerRef.IsInLocation(LocMountainsObservatoryIntLocation)" in patch
    assert "EnsureAliasContainerItem(Alias_Actor_RadioTowerCorpse, BS01_MQ07_Over_RadioTowerNote, Alias_QO_Book_RadioTowerNote)" in patch
    assert "SetAliasEnabled(Alias_Activator_RadioTowerLaserGrid, False)" in patch
    assert "EnsureAliasContainerItem(Alias_Actor_KeycardCorpse, BS01_MQ07_HoldingCellKeyCard01, Alias_QO_Keycard)" in patch
    assert "playerRef.AddKeyword(BS01_MQ07_Over_HandscanRegistrationKeyword)" in patch
    assert "SetLocalPlayerValue(BS01_MQ07_Over_AV_MainframeAccess, 1.0)" in patch
    assert "StartCafeteriaEncounter()" in patch
    assert "RegisterForRemoteEvent(enemyRef, \"OnDeath\")" in patch
    assert "Event Actor.OnDeath(Actor akSender, Actor akKiller)" in patch
    assert "StartBoundActorCombat(Alias_Actor_SentryBot)" in patch

    _assert_in_order(
        patch,
        "SetObjectiveDisplayed(5, True, True)",
        "SetObjectiveDisplayed(30, True, True)",
        "SetAliasEnabled(Alias_Activator_RadioTowerLaserGrid, False)",
        "SaySODUSTopic(pSODUSBiohazard)",
        "EnsureAliasContainerItem(Alias_Actor_KeycardCorpse, BS01_MQ07_HoldingCellKeyCard01, Alias_QO_Keycard)",
        "    StartCafeteriaEncounter()",
        "SetLocalPlayerValue(BS01_MQ07_Over_AV_MainframeAccess, 1.0)",
        "StartBoundActorCombat(Alias_Actor_SentryBot)",
        "SetObjectiveDisplayed(160, True, True)",
        "SetObjectiveDisplayed(170, True, True)",
    )


def test_bs01_mq07_cafeteria_requires_spawned_enemies_to_die_before_progression():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    enemies = _member_source(patch, "ResolveCafeteriaEnemies")
    resolver = _member_source(patch, "ResolveCafeteriaSpawnCenter")
    spawn = _member_source(patch, "SpawnCafeteriaEnemy")
    populate = _member_source(patch, "PopulateCafeteriaEncounter")
    start = _member_source(patch, "StartCafeteriaEncounter")
    cleanup = _member_source(patch, "CleanupCafeteriaEncounter")
    complete = _member_source(patch, "CompleteCafeteriaEncounterIfCleared")
    on_death = _member_source(patch, "Actor.OnDeath")
    final_stage = _member_source(patch, "Fragment_Stage_9000_Item_00")

    assert "Return GetAlias(64) as RefCollectionAlias" in enemies
    assert 'Game.GetFormFromFile(0x005DCA76, "SeventySix.esm")' in resolver
    assert 'Game.GetFormFromFile(0x005E52EC, "SeventySix.esm") as ActorBase' in populate
    assert 'Game.GetFormFromFile(0x005E52EB, "SeventySix.esm") as ActorBase' in populate
    assert 'Game.GetFormFromFile(0x005DCFB9, "SeventySix.esm") as ActorBase' in populate
    assert populate.count("SpawnCafeteriaEnemy(spawnCenter, enemies,") == 3

    _assert_in_order(
        spawn,
        "spawnCenter.PlaceAtMe(enemyBase, 1, True, False, False)",
        "enemies.AddRef(enemyRef)",
    )
    assert start.count("HasCafeteriaEnemyReferences(enemies)") == 2
    assert "PopulateCafeteriaEncounter(spawnCenter, enemies)" in start
    assert "If !HasCafeteriaEnemyReferences(enemies)\n        Return\n    EndIf" in start
    assert "If IsStageDone(2500)\n        CleanupCafeteriaEncounter(enemies)\n        Return\n    EndIf" in start
    assert start.index("If IsStageDone(2500)") < start.index("PopulateCafeteriaEncounter")
    assert "SetStage(2500)" not in start
    assert "enemyRef.Delete()" not in spawn
    assert "enemyRef.Delete()" not in populate
    assert "enemyRef.Delete()" not in start

    assert "If enemies == None || enemies.GetCount() == 0\n        Return\n    EndIf" in complete
    assert "Bool observedEnemy = False" in complete
    assert "observedEnemy = True" in complete
    assert "If !enemyRef.IsDead()\n                Return" in complete
    assert "If observedEnemy\n        SetStage(2500)" in complete
    assert complete.count("SetStage(2500)") == 1

    assert "Int enemyIndex = enemies.GetCount() - 1" in cleanup
    assert "While enemyIndex >= 0" in cleanup
    assert "enemyIndex -= 1" in cleanup
    _assert_in_order(
        cleanup,
        # OnDeath is an Actor event: stock PapyrusCompiler.exe rejects an
        # ObjectReference receiver, so the cleanup unregisters the Actor cast.
        "Actor enemyActor = enemyRef as Actor",
        'UnregisterForRemoteEvent(enemyActor, "OnDeath")',
        "enemies.RemoveRef(enemyRef)",
        "enemyRef.Disable(False)",
        "enemyRef.Delete()",
    )
    _assert_in_order(
        complete,
        "If observedEnemy",
        "SetStage(2500)",
        "CleanupCafeteriaEncounter(enemies)",
    )
    assert "CleanupCafeteriaEncounter(ResolveCafeteriaEnemies())" in final_stage
    assert patch.count("CleanupCafeteriaEncounter(") == 4
    assert patch.count("enemyRef.Delete()") == 1
    assert "CompleteCafeteriaEncounterIfCleared()" in on_death


def test_bs01_mq07_patch_preserves_choice_transmitter_and_story_handoff():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    reconcile = _member_source(patch, "ReconcileBestDefensePlayerAlias")
    handoff = _member_source(patch, "SendBestDefenseStoryEvent")
    retry = _member_source(patch, "OnTimer")
    final_stage = _member_source(patch, "Fragment_Stage_9000_Item_00")

    assert "SetLocalPlayerValue(BS01_AV_LoyaltyChoice, 1.0)" in patch
    assert "SetLocalPlayerValue(BS01_AV_LoyaltyChoice, 2.0)" in patch
    assert "SetLocalPlayerValue(BS01_AV_LoyaltyChoice, 0.0)" in patch
    assert patch.count("SetStage(4000)") == 3

    assert "transmitterScript.DestroyTransmitter()" in patch
    assert "transmitterScript.GoToState(\"broken\")" in patch
    assert "DoDestructionFX" not in patch
    assert "BoS_Transmitter_Explosion" not in patch
    assert patch.count("DestroyBoundTransmitter()") == 2

    assert handoff.count("BS01_MQ08_Defense_QuestStartKeyword.SendStoryEventAndWait") == 1
    assert "Bool questStarted = BS01_MQ08_Defense_QuestStartKeyword.SendStoryEventAndWait" in handoff
    assert "If BS01_MQ08_Defense.IsCompleted()\n        CancelTimer(0)\n        Return\n    EndIf" in handoff
    assert "If BS01_MQ08_Defense.IsRunning()\n        ReconcileBestDefensePlayerAlias(playerRef)\n        CancelTimer(0)\n        Return\n    EndIf" in handoff
    assert "If questStarted || BS01_MQ08_Defense.IsRunning()\n        ReconcileBestDefensePlayerAlias(playerRef)\n        CancelTimer(0)" in handoff
    assert "ElseIf BS01_MQ08_Defense.IsCompleted()\n        CancelTimer(0)\n    Else\n        StartTimer(5.0, 0)" in handoff
    assert "BS01_MQ08_Defense.Start()" not in patch
    assert "nextPlayerAlias.ForceRefTo(playerRef)" in reconcile
    assert "Event OnTimer(Int aiTimerID)" in retry
    assert "If aiTimerID == 0 && IsStageDone(9000)\n        SendBestDefenseStoryEvent()\n    ElseIf aiTimerID == 0\n        CancelTimer(0)" in retry
    assert "RegisterForSingleUpdate" not in patch
    assert "UnregisterForUpdate" not in patch
    _assert_in_order(
        final_stage,
        "CleanupCafeteriaEncounter(ResolveCafeteriaEnemies())",
        "SendBestDefenseStoryEvent()",
    )
    assert "BS01_MQ07_Over.Stop()" not in patch
    assert "SetAliasEnabled(Alias_SoundMarker_EmergencyAlert, True)" in patch
    assert patch.count("SetAliasEnabled(Alias_SoundMarker_EmergencyAlert, False)") == 2
