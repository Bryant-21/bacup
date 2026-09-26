Int Function FindObjectiveRow(Int aiObjectiveID)
    If ObjectiveData == None
        Return -1
    EndIf
    Int row = 0
    While row < ObjectiveData.Length
        If ObjectiveData[row].ObjectiveID == aiObjectiveID
            Return row
        EndIf
        row += 1
    EndWhile
    Return -1
EndFunction

Function EnsureObjectiveState()
    If B21RunningObjectives == None
        B21RunningObjectives = New Int[0]
        B21ObjectiveRemaining = New Float[0]
    EndIf
EndFunction

Bool Function IsObjectiveRunning(Int aiObjectiveID)
    EnsureObjectiveState()
    Return B21RunningObjectives.Find(aiObjectiveID) >= 0
EndFunction

Float Function GetObjectiveTimeRemaining(Int aiObjectiveID)
    EnsureObjectiveState()
    Int slot = B21RunningObjectives.Find(aiObjectiveID)
    If slot < 0
        Return 0.0
    EndIf
    Return B21ObjectiveRemaining[slot]
EndFunction

Function StartObjective(Int aiObjectiveID)
    Int row = FindObjectiveRow(aiObjectiveID)
    If row < 0 || IsStopping() || IsStopped() || IsCompleted()
        Return
    EndIf
    EnsureObjectiveState()

    ObjectiveDatum datum = ObjectiveData[row]
    If datum.OnStart_SetStage >= 0 && !IsStageDone(datum.OnStart_SetStage)
        SetStage(datum.OnStart_SetStage)
    EndIf
    SetObjectiveCompleted(aiObjectiveID, False)
    SetObjectiveFailed(aiObjectiveID, False)
    SetObjectiveDisplayed(aiObjectiveID, True, datum.OnStart_ForceRedisplay || datum.OnStart_SetObjectiveAnnounceState == 1)

    Int slot = B21RunningObjectives.Find(aiObjectiveID)
    If slot < 0
        B21RunningObjectives.Add(aiObjectiveID)
        B21ObjectiveRemaining.Add(datum.TimeLimit)
    Else
        B21ObjectiveRemaining[slot] = datum.TimeLimit
    EndIf
    If datum.TimeLimit > 0.0
        StartTimer(1.0, 24431)
    EndIf
EndFunction

Function EndObjective(Int aiObjectiveID)
    Int row = FindObjectiveRow(aiObjectiveID)
    If row < 0 || EndObjectiveSpinLock
        Return
    EndIf
    EndObjectiveSpinLock = True
    EnsureObjectiveState()
    Int slot = B21RunningObjectives.Find(aiObjectiveID)
    If slot >= 0
        B21RunningObjectives.Remove(slot)
        B21ObjectiveRemaining.Remove(slot)
    EndIf

    ObjectiveDatum datum = ObjectiveData[row]
    ; FO76's completion states: 0 clears the objective, 1 completes it, anything else fails it.
    If datum.OnEnd_SetObjectiveCompletionState == 0
        SetObjectiveDisplayed(aiObjectiveID, False)
    ElseIf datum.OnEnd_SetObjectiveCompletionState == 1
        SetObjectiveCompleted(aiObjectiveID, True)
    Else
        SetObjectiveFailed(aiObjectiveID, True)
    EndIf
    If datum.OnEnd_SetStage >= 0
        SetStage(datum.OnEnd_SetStage)
    EndIf
    EndObjectiveSpinLock = False

    If datum.OnEnd_StartObjective >= 0
        StartObjective(datum.OnEnd_StartObjective)
    EndIf
    Var[] args = New Var[1]
    args[0] = aiObjectiveID
    SendCustomEvent("ObjectiveEnded", args)
EndFunction

Function StopAllObjectives()
    CancelTimer(24431)
    EnsureObjectiveState()
    B21RunningObjectives = New Int[0]
    B21ObjectiveRemaining = New Float[0]
EndFunction

Event OnQuestInit()
    EndObjectiveSpinLock = False
    B21RunningObjectives = None
    B21ObjectiveRemaining = None
    EnsureObjectiveState()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 24431
        Return
    EndIf
    If IsStopping() || IsStopped() || IsCompleted()
        Return
    EndIf
    EnsureObjectiveState()

    Int expired = -1
    Int slot = 0
    While slot < B21RunningObjectives.Length
        If B21ObjectiveRemaining[slot] > 0.0
            B21ObjectiveRemaining[slot] = B21ObjectiveRemaining[slot] - 1.0
            If B21ObjectiveRemaining[slot] <= 0.0 && expired < 0
                expired = B21RunningObjectives[slot]
            EndIf
        EndIf
        slot += 1
    EndWhile

    If expired >= 0
        EndObjective(expired)
    EndIf
    If B21RunningObjectives.Length > 0
        StartTimer(1.0, 24431)
    EndIf
EndEvent

Event OnQuestShutdown()
    StopAllObjectives()
EndEvent
