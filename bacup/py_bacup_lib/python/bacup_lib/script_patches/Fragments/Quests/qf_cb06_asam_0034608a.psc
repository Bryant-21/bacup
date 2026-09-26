Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Bool Function IsASAMDown()
    If Alias_ASAM == None
        Return False
    EndIf
    Actor asamActor = Alias_ASAM.GetActorReference()
    Return asamActor != None && asamActor.IsDead()
EndFunction

Function ScheduleEventShutdown(Float afSeconds)
    CancelTimer(34605)
    StartTimer(afSeconds, 34605)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 34605 && !IsStageDone(9999)
        SetStage(9999)
    EndIf
EndEvent

Function Fragment_Stage_0010_Item_00()
    ; Repairing the turret closes the repair objective, which disarms its 30 s timer.
    CompleteOpenObjective(20)
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0020_Item_00()
    If !IsASAMDown()
        Return
    EndIf
    ResetEventObjective(20)
    SetObjectiveDisplayed(20, True, True)
EndFunction

Function Fragment_Stage_0900_Item_00()
    CompleteOpenObjective(10)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteOpenObjective(10)
    CompleteOpenObjective(20)
    ScheduleEventShutdown(10.0)
EndFunction

Function Fragment_Stage_9500_Item_00()
    FailOpenObjective(10)
    FailOpenObjective(20)
    ScheduleEventShutdown(10.0)
EndFunction

Function Fragment_Stage_9999_Item_00()
    CancelTimer(34605)
    Stop()
EndFunction
