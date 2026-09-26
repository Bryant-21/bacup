Event OnAliasInit()
    myInst = GetOwningQuest()
    bInitComplete = False
    iTotalTargets = 0
    If myInst != None
        RegisterForRemoteEvent(myInst, "OnStageSet")
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    ReconcileTargets()
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == myInst
        ReconcileTargets()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        ReconcileTargets()
    EndIf
EndEvent

Event OnDying(ObjectReference akSenderRef, Actor akKiller)
    If UseOnDyingInstead && akSenderRef != None && Find(akSenderRef) >= 0
        ReconcileTargets(akSenderRef)
    EndIf
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    If !UseOnDyingInstead && akSenderRef != None && Find(akSenderRef) >= 0
        ReconcileTargets(akSenderRef)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 8877
        ReconcileTargets()
    EndIf
EndEvent

Function ReconcileTargets(ObjectReference akDyingRef = None)
    If myInst == None
        myInst = GetOwningQuest()
    EndIf
    If myInst == None || !myInst.IsRunning() || myInst.IsStopping() || myInst.IsCompleted()
        CancelTimer(8877)
        Return
    EndIf
    If iStageToSetOnAllKilled >= 0 && myInst.IsStageDone(iStageToSetOnAllKilled)
        CancelTimer(8877)
        Return
    EndIf
    If iObjectiveIndex >= 0 && myInst.IsObjectiveFailed(iObjectiveIndex)
        CancelTimer(8877)
        Return
    EndIf
    If iStageToBeginTracking > 0 && !myInst.IsStageDone(iStageToBeginTracking)
        Return
    EndIf

    Int living = 0
    Int total = 0
    Int index = 0
    While index < GetCount()
        Actor target = GetAt(index) as Actor
        If target != None
            total += 1
            If target != akDyingRef && !target.IsDead()
                living += 1
            EndIf
        EndIf
        index += 1
    EndWhile
    iTotalTargets = total
    If total <= 0
        StartTimer(1.0, 8877)
        Return
    EndIf
    If !bInitComplete
        bInitComplete = True
        If iObjectiveIndex >= 0
            myInst.SetObjectiveDisplayed(iObjectiveIndex, True)
        EndIf
    EndIf
    index = 0
    While StagesToSetOnRemaining != None && index < StagesToSetOnRemaining.Length
        StageToSetOnRemaining threshold = StagesToSetOnRemaining[index]
        If living <= threshold.iEnemiesRemaining && threshold.iStageToSet >= 0 && !myInst.IsStageDone(threshold.iStageToSet)
            myInst.SetStage(threshold.iStageToSet)
        EndIf
        index += 1
    EndWhile
    If living == 0
        CancelTimer(8877)
        If iObjectiveIndex >= 0
            myInst.SetObjectiveCompleted(iObjectiveIndex, True)
        EndIf
        If iStageToSetOnAllKilled >= 0 && !myInst.IsStageDone(iStageToSetOnAllKilled)
            myInst.SetStage(iStageToSetOnAllKilled)
        EndIf
    Else
        StartTimer(1.0, 8877)
    EndIf
EndFunction

Event OnAliasShutdown()
    CancelTimer(8877)
    If myInst != None
        UnregisterForRemoteEvent(myInst, "OnStageSet")
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    bInitComplete = False
EndEvent
