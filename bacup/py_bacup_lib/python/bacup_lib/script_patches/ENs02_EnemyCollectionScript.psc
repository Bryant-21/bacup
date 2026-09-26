; Bound only to Dropped Connection's ActiveEnemies alias 13 (prerequisite 90,
; all dead 95). iTriggerStageOnEnemiesRemaining has no consumer in the bound
; data and is left unused. Empty collections never count as a victory.
Event OnAliasInit()
    Quest owner = GetOwningQuest()
    If owner != None
        RegisterForRemoteEvent(owner, "OnStageSet")
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    ENs02_CheckEnemies()
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == GetOwningQuest()
        ENs02_CheckEnemies()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        ENs02_CheckEnemies()
    EndIf
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    If akSenderRef != None && Find(akSenderRef) >= 0
        ENs02_CheckEnemies(akSenderRef)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 3
        ENs02_CheckEnemies()
    EndIf
EndEvent

Function ENs02_CheckEnemies(ObjectReference akDyingRef = None)
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning() || owner.IsStopping() || owner.IsCompleted() || owner.IsStageDone(iAllEnemiesDead)
        CancelTimer(3)
        Return
    EndIf
    If iPreReqStage > 0 && !owner.IsStageDone(iPreReqStage)
        Return
    EndIf
    Int total = 0
    Int living = 0
    Int index = 0
    While index < GetCount()
        Actor enemy = GetAt(index) as Actor
        If enemy != None
            total += 1
            If enemy != akDyingRef && !enemy.IsDead()
                living += 1
            EndIf
        EndIf
        index += 1
    EndWhile
    If total > 0 && living == 0
        CancelTimer(3)
        owner.SetStage(iAllEnemiesDead)
        Return
    EndIf
    StartTimer(1.0, 3)
EndFunction

Event OnAliasShutdown()
    CancelTimer(3)
    Quest owner = GetOwningQuest()
    If owner != None
        UnregisterForRemoteEvent(owner, "OnStageSet")
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndEvent
