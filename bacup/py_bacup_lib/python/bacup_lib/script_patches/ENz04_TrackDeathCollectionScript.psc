Event OnAliasInit()
    bCooldownActive = False
    Quest eventQuest = GetOwningQuest()
    If eventQuest != None
        RegisterForRemoteEvent(eventQuest, "OnStageSet")
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    CheckPatrol()
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == GetOwningQuest()
        CheckPatrol()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        CheckPatrol()
    EndIf
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    If akSenderRef != None && Find(akSenderRef) >= 0
        CheckPatrol(akSenderRef)
    EndIf
EndEvent

Function CheckPatrol(ObjectReference akLostBot = None)
    Quest eventQuest = GetOwningQuest()
    If eventQuest == None || !eventQuest.IsRunning() || eventQuest.IsCompleted() || eventQuest.IsStopping()
        CancelTimer(2)
        Return
    EndIf
    If eventQuest.IsStageDone(iTimerRanOutStage) || eventQuest.IsStageDone(iFailureStage) || eventQuest.IsStageDone(iShutDownStage)
        CancelTimer(2)
        Return
    EndIf
    If !eventQuest.IsStageDone(20)
        Return
    EndIf
    Int total = 0
    Int living = 0
    Int index = 0
    While index < GetCount()
        Actor patrolBot = GetAt(index) as Actor
        If patrolBot != None
            total += 1
            If patrolBot != akLostBot && !patrolBot.IsDead()
                living += 1
            EndIf
        EndIf
        index += 1
    EndWhile
    ObjectReference speaker
    If SpeakingTerminal != None
        speaker = SpeakingTerminal.GetReference()
    EndIf
    If total > 0 && living == 0
        CancelTimer(2)
        If speaker != None && ENz04_AllPatrollersLost != None
            speaker.Say(ENz04_AllPatrollersLost)
        EndIf
        If !eventQuest.IsStageDone(iFailureStage)
            eventQuest.SetStage(iFailureStage)
        EndIf
        Return
    EndIf
    If akLostBot != None && !bCooldownActive && speaker != None && ENz04_PatrollerLost != None
        bCooldownActive = True
        speaker.Say(ENz04_PatrollerLost)
        StartTimer(iAudioCooldownLength as Float, iAudioCooldownID)
    EndIf
    StartTimer(1.0, 2)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == iAudioCooldownID
        bCooldownActive = False
    ElseIf aiTimerID == 2
        CheckPatrol()
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(2)
    CancelTimer(iAudioCooldownID)
    bCooldownActive = False
    Quest eventQuest = GetOwningQuest()
    If eventQuest != None
        UnregisterForRemoteEvent(eventQuest, "OnStageSet")
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndEvent
