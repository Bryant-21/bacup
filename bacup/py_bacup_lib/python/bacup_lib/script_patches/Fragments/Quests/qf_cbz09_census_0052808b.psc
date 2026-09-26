CBZ09_QuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as CBZ09_QuestScript
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(0)
    ResetEventObjective(300)
EndFunction

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

Function FailOpenObjectives()
    FailOpenObjective(0)
    FailOpenObjective(300)
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function ReleaseRobotFromCaptivity()
    ; CaptiveFaction is "friends with everyone": the census taker only becomes a
    ; target for the wave enemies once it starts its rounds.
    If Alias_Robot == None || CaptiveFaction == None
        Return
    EndIf
    Actor robotActor = Alias_Robot.GetActorReference()
    If robotActor != None && robotActor.IsInFaction(CaptiveFaction)
        robotActor.RemoveFromFaction(CaptiveFaction)
    EndIf
EndFunction

Function StopEventWaves(Bool abRemoveActors)
    CBZ09_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StopDefendWaves(abRemoveActors)
    EndIf
EndFunction

Function ScheduleEventShutdown(Float afSeconds)
    CancelTimer(52086)
    StartTimer(afSeconds, 52086)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 52086
        Stop()
    EndIf
EndEvent

Function Fragment_Stage_0000_Item_00()
    ResetEventObjectives()
    SetObjectiveDisplayed(0, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    CompleteOpenObjective(0)
    ReleaseRobotFromCaptivity()
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300, True, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteOpenObjective(0)
    CompleteOpenObjective(300)
    StopEventWaves(False)
    CBZ09_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.SayShutdownTopic()
    EndIf
    ScheduleEventShutdown(10.0)
EndFunction

Function Fragment_Stage_9500_Item_00()
    FailOpenObjectives()
    StopEventWaves(False)
    ScheduleEventShutdown(10.0)
EndFunction

Function Fragment_Stage_9991_Item_00()
    FailOpenObjectives()
    StopEventWaves(True)
    ScheduleEventShutdown(5.0)
EndFunction
