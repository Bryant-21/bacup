Function ResetEventObjectives()
    SetObjectiveDisplayed(100, False)
    SetObjectiveCompleted(100, False)
    SetObjectiveFailed(100, False)
    SetObjectiveDisplayed(200, False)
    SetObjectiveCompleted(200, False)
    SetObjectiveFailed(200, False)
    SetObjectiveDisplayed(300, False)
    SetObjectiveCompleted(300, False)
    SetObjectiveFailed(300, False)
EndFunction

Function PublishLocationGlobal()
    If FSS02_Vigilant_LocationGlobal == None
        Return
    EndIf

    Location chosenTower = None
    If ChosenRelayTowerLocation != None
        chosenTower = ChosenRelayTowerLocation.GetLocation()
    EndIf

    Float towerIndex = 0.0
    Int row = 0
    While LocAndGlobalSet != None && row < LocAndGlobalSet.Length
        If chosenTower != None && LocAndGlobalSet[row].EventLocation == chosenTower
            towerIndex = LocAndGlobalSet[row].LocationGlobal as Float
        EndIf
        row += 1
    EndWhile
    FSS02_Vigilant_LocationGlobal.SetValue(towerIndex)
EndFunction

Function ArmRoverSetupFailsafe()
    CancelTimer(7302)
    StartTimer(15.0, 7302)
EndFunction

Function ArmEventShutdown()
    Float delay = QuestStopFailsafeTimer
    If delay < 1.0
        delay = 1.0
    EndIf
    CancelTimer(TimerID_QuestStopFailsafe)
    StartTimer(delay, TimerID_QuestStopFailsafe)
EndFunction

Function ShutdownEvent()
    CancelTimer(TimerID_QuestStopFailsafe)
    If !IsStopping() && !IsStopped()
        Stop()
    EndIf
EndFunction

Event OnQuestInit()
    PublishLocationGlobal()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 50 && !IsStageDone(100)
        ArmRoverSetupFailsafe()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 7302
        If IsRunning() && IsStageDone(50) && !IsStageDone(100)
            SetStage(100)
        EndIf
    ElseIf aiTimerID == TimerID_QuestStopFailsafe
        ShutdownEvent()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(7302)
    CancelTimer(TimerID_QuestStopFailsafe)
    If FSS02_Vigilant_LocationGlobal != None
        FSS02_Vigilant_LocationGlobal.SetValue(0.0)
    EndIf
EndEvent
