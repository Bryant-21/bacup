Function SetMineAlarm(Bool abEnabled)
    Int index = 0
    If Klaxons != None
        While index < Klaxons.GetCount()
            Default2StateActivator alarmLight = Klaxons.GetAt(index) as Default2StateActivator
            If alarmLight != None
                alarmLight.SetOpen(abEnabled)
            EndIf
            index += 1
        EndWhile
    EndIf
    index = 0
    If AudioMarkers != None
        While index < AudioMarkers.GetCount()
            ObjectReference marker = AudioMarkers.GetAt(index)
            If marker != None
                If abEnabled
                    marker.EnableNoWait()
                Else
                    marker.DisableNoWait()
                EndIf
            EndIf
            index += 1
        EndWhile
    EndIf
EndFunction

Function StopMineAlarm()
    CancelTimer(iTimerID)
    SetMineAlarm(False)
    UnregisterForAllRemoteEvents()
    Stop()
EndFunction

Event OnQuestInit()
    iCurrentCount = -1
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    Quest brigade = Game.GetFormFromFile(0x0003363B, "SeventySix.esm") as Quest
    If brigade != None
        RegisterForRemoteEvent(brigade, "OnStageSet")
    EndIf
    ReconcileMineAlarm()
EndEvent

Function ReconcileMineAlarm()
    Quest brigade = Game.GetFormFromFile(0x0003363B, "SeventySix.esm") as Quest
    If !IsRunning() || brigade == None
        Return
    EndIf
    If !brigade.IsRunning() || brigade.GetStage() >= 70
        StopMineAlarm()
    ElseIf brigade.IsStageDone(60)
        If iCurrentCount < 0
            iCurrentCount = 0
        EndIf
        SetMineAlarm(True)
        StartTimer(iTimerInterval, iTimerID)
    Else
        iCurrentCount = -1
        CancelTimer(iTimerID)
        SetMineAlarm(False)
    EndIf
EndFunction

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    ReconcileMineAlarm()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != iTimerID || !IsRunning() || iCurrentCount < 0
        Return
    EndIf
    iCurrentCount += 1
    If iCurrentCount >= iFailsafeCount
        StopMineAlarm()
    Else
        StartTimer(iTimerInterval, iTimerID)
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    ReconcileMineAlarm()
EndEvent

Event OnQuestShutdown()
    CancelTimer(iTimerID)
    SetMineAlarm(False)
    UnregisterForAllRemoteEvents()
EndEvent
