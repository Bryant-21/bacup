Actor Function ASAMActor()
    If ASAM == None
        Return None
    EndIf
    Return ASAM.GetActorReference()
EndFunction

Bool Function IsEventResolved()
    If IsStopping() || IsStopped() || IsCompleted()
        Return True
    EndIf
    Return IsStageDone(9000) || IsStageDone(9500)
EndFunction

Function WatchASAM(Bool abWatch)
    Actor asamActor = ASAMActor()
    If asamActor == None
        Return
    EndIf
    If abWatch
        RegisterForRemoteEvent(asamActor, "OnDeath")
        RegisterForRemoteEvent(asamActor, "OnActivate")
    Else
        UnregisterForRemoteEvent(asamActor, "OnDeath")
        UnregisterForRemoteEvent(asamActor, "OnActivate")
    EndIf
EndFunction

Function StopEventWaves(Bool abRemoveActors)
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
    If waves != None
        waves.StopAllEncounterWaves(abRemoveActors)
    EndIf
EndFunction

Function StartEventWaves()
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
    If waves != None
        waves.StartEncounterWaveByID("Endless trickle of scorched")
    EndIf
EndFunction

Function BeginDefense()
    If FirstPlayerJoined || IsEventResolved()
        Return
    EndIf
    Actor asamActor = ASAMActor()
    If asamActor == None
        StartTimer(2.0, 34601)
        Return
    EndIf

    FirstPlayerJoined = True
    ; The turret ships in CaptiveFaction ("friends with everyone") so nothing shoots it
    ; outside the activity; the scorchbeast and its Scorched need it attackable.
    If CaptiveFaction != None && asamActor.IsInFaction(CaptiveFaction)
        asamActor.RemoveFromFaction(CaptiveFaction)
    EndIf
    WatchASAM(True)

    If ScorchbeastArrivesStage >= 0 && !IsStageDone(ScorchbeastArrivesStage)
        SetStage(ScorchbeastArrivesStage)
    EndIf
    If StageToSetDefendObjective >= 0
        SetStage(StageToSetDefendObjective)
    EndIf
EndFunction

Function RepairASAM()
    Actor asamActor = ASAMActor()
    If asamActor == None || IsEventResolved()
        Return
    EndIf
    If !asamActor.IsDead()
        Return
    EndIf
    ; FO76 repaired the ASAM from a workshop repair recipe; activating the wreck is the
    ; single-player stand-in, and the 30 s repair objective disarms as soon as it closes.
    asamActor.Resurrect()
    If CaptiveFaction != None && asamActor.IsInFaction(CaptiveFaction)
        asamActor.RemoveFromFaction(CaptiveFaction)
    EndIf
    WatchASAM(True)
    If StageToSetDefendObjective >= 0
        SetStage(StageToSetDefendObjective)
    EndIf
EndFunction

Event OnQuestInit()
    FirstPlayerJoined = False
    StartTimer(2.0, 34601)
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == StageToSetDefendObjective
        StartEventWaves()
    EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If akSender != ASAMActor() || IsEventResolved()
        Return
    EndIf
    If StageToSetRepairObjective >= 0
        SetStage(StageToSetRepairObjective)
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akSender != ASAMActor() || akActionRef != Game.GetPlayer()
        Return
    EndIf
    RepairASAM()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 34601
        Return
    EndIf
    If IsEventResolved() || !IsRunning()
        Return
    EndIf
    If ASAMActor() == None
        StartTimer(2.0, 34601)
        Return
    EndIf
    BeginDefense()
EndEvent

Event OnQuestShutdown()
    CancelTimer(34601)
    WatchASAM(False)
    StopEventWaves(True)
    Int index = 0
    While ScorchbeastAllies != None && index < ScorchbeastAllies.GetCount()
        Actor ally = ScorchbeastAllies.GetAt(index) as Actor
        If ally != None && !ally.IsDead()
            ally.DisableNoWait()
            ally.Delete()
        EndIf
        index += 1
    EndWhile
EndEvent
