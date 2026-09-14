Event OnQuestInit()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && VS_MiscTempFlag != None
        playerRef.SetValue(VS_MiscTempFlag, 1.0)
    EndIf
    If !IsStageDone(10)
        SetStage(10)
    EndIf
    StartTimer(1.0, 1)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1 || IsCompleted()
        Return
    EndIf
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference targetRef = Alias_SwarmQuestTarget.GetReference()
    If playerRef == None || targetRef == None
        StartTimer(1.0, 1)
        Return
    EndIf

    Float distance = playerRef.GetDistance(targetRef)
    If !IsStageDone(StartObjectiveStage) && distance <= MaxDistanceToStart
        SetStage(StartObjectiveStage)
    EndIf
    If IsStageDone(StartObjectiveStage) && distance <= DistanceToCompleteObjective
        SetStage(CompleteObjectiveStage)
    Else
        StartTimer(1.0, 1)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && VS_MiscTempFlag != None
        playerRef.SetValue(VS_MiscTempFlag, 0.0)
    EndIf
EndEvent
