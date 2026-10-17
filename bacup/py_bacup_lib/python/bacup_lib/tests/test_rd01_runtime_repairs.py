from pathlib import Path
import re

from bacup_lib.workflows.unified import _merge_script_method_patches


PATCHES = Path(__file__).parents[1] / "script_patches/Raids"


def merged(relative, declarations, members):
    skeleton = declarations + "\n" + "\n".join(members)
    result = _merge_script_method_patches(skeleton, (PATCHES / relative).read_text())
    assert "Debug.Trace(\"stub\")" not in result
    assert declarations in result
    return result


def body(source, kind, name):
    matches = re.findall(rf"\b{kind} {name}\([^\n]*\)\s*(.*?)End{kind}", source, re.S)
    assert len(matches) == 1, name
    return matches[0]


def test_guardian_startup_and_release_are_cancelled_by_cleanup_or_death():
    source = merged("RD01/Enc01/QuestScript.psc", """Scriptname Raids:RD01:Enc01:QuestScript Extends Raids:BaseEncounterScript
Int Phase = 0
Int IntroAmbushFurnTimerID = 6
Actor BossActor
ReferenceAlias Property Alias_Actor_Boss Auto Mandatory""", [
        'Function StartEncounter()\nDebug.Trace("stub")\nEndFunction',
        'Function ReleaseBoss()\nDebug.Trace("stub")\nEndFunction',
        'Event OnTimer(Int aiTimerID)\nDebug.Trace("stub")\nEndEvent',
    ])
    start = body(source, "Function", "StartEncounter")
    assert 'Phase = -1' in start and 'StartTimer(0.1, 15)' in start
    assert 'EncounterStartScene.Start()' not in start
    timer = body(source, "Event", "OnTimer")
    deferred = timer.split('If aiTimerID == 14')[0]
    assert 'If Phase == -1' in deferred and 'If IsStarting()' in deferred
    assert 'ElseIf IsRunning() && !IsStageDone(9000)' in deferred
    assert 'BeginEncounter()' in deferred
    assert 'CancelTimer(15)' in body(source, "Function", "CancelEncounterTimers")
    assert 'CancelEncounterTimers()' in body(source, "Function", "CleanupSpawned")
    begin = body(source, "Function", "BeginEncounter")
    assert begin.index('BossActor.WaitFor3DLoad()') < begin.index('EncounterStartScene.Start()')
    release = body(source, "Function", "ReleaseBoss")
    before_wait, after_wait = release.split('BossActor.WaitFor3DLoad()', 1)
    for guard in ('Phase <= 0', '!IsRunning()', 'IsStageDone(9000)', 'BossActor.IsDead()', 'BossActor.IsDisabled()'):
        assert guard in before_wait and guard in after_wait
    assert begin.index('StartTimer(ambushTime, IntroAmbushFurnTimerID)') < begin.index('gunAnimations.AnimateOpenUpAmbush()')
    # The rise must start after the base has opened, not alongside it.
    assert 'PlayIdleAction' not in begin
    assert 'Game.GetFormFromFile(0x02248D, "Fallout4.esm") as Action' in release
    assert release.index('BossActor.PlayIdleAction(exitAction)') < release.index('StartCombat(player)')
    assert 'PlaySubGraphAnimation("AmbushExit")' not in source
    assert 'exit accepted=' in release
    assert 'BossActor.GetPositionZ()' in release
    assert release.index('EnableAI(True)') < release.index('StartCombat(player)')
    assert release.index('StartCombat(player)') < release.index('EnterShielded()')
    assert release.index('StartCombat(player)') < release.index('SetTriggersEnabled(True)')
    assert release.index('StartCombat(player)') < release.index('SetFloorsFx("GoToFog")')


def test_ambush_snap_checks_disabled_and_rechecks_after_latent_load():
    source = merged("SnapToFurnitureOn3DLoad.psc", """Scriptname Raids:SnapToFurnitureOn3DLoad Extends Actor
ActorValue Property AmbushRelease Auto Mandatory
Keyword Property LinkAmbushFurniture Auto Mandatory""", [
        'Event OnLoad()\nDebug.Trace("stub")\nEndEvent',
    ])
    on_load = body(source, "Event", "OnLoad")
    before_wait, after_wait = on_load.split('WaitFor3DLoad()', 1)
    for guard in ('IsDisabled()', 'IsDead()', 'GetValue(AmbushRelease) > 0.0'):
        assert guard in before_wait and guard in after_wait
    assert after_wait.index('IsDisabled()') < after_wait.index('SnapIntoInteraction(ambushFurniture)')


def test_drill_fuel_access_opens_at_active_phase_and_resets_on_retry():
    source = merged("RD01/Enc02/QuestScript.psc", """Scriptname Raids:RD01:Enc02:QuestScript Extends Raids:BaseEncounterScript
Int Phase = 0
Int iCurrentFuelDeposited = 0
RefCollectionAlias Property Alias_CaveDoors Auto Mandatory
ReferenceAlias Property Alias_Activator_FuelDepositPoint Auto Mandatory""", [
        'Function BeginActivePhase()\nDebug.Trace("stub")\nEndFunction',
        'Function CleanupSpawned()\nDebug.Trace("stub")\nEndFunction',
    ])
    active = body(source, "Function", "BeginActivePhase")
    assert active.index('Phase != 1') < active.index('SetCaveDoorsAccessible(True)')
    assert 'IsStageDone(9999)' in active and 'IsStageDone(iQuestCompletionStage)' in active
    assert 'SetCaveDoorsAccessible(False)' in body(source, "Function", "CleanupSpawned")
    doors = body(source, "Function", "SetCaveDoorsAccessible")
    unlocked, locked = doors.split('Else', 1)
    assert 'caveDoor.SetLockLevel(0)' in unlocked and 'caveDoor.Lock(False)' in unlocked
    assert 'caveDoor.SetLockLevel(254)' in locked and 'caveDoor.Lock(True)' in locked
    assert 'caveDoor.SetOpen(False)' in locked
    deposit = body(source, "Event", r'ReferenceAlias\.OnActivate')
    assert deposit.index('player.GetItemCount(kFuelObject) < 1') < deposit.index('player.RemoveItem(kFuelObject, 1, True)')
    assert deposit.index('iCurrentFuelDeposited += 1') < deposit.index('UpdateDrillProgress(fuelNeeded)')
    assert 'If iCurrentFuelDeposited >= fuelNeeded' in deposit and 'BeginOutro()' in deposit
