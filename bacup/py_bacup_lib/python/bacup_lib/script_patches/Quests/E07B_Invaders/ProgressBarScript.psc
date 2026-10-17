; FO4 has no objective progress bars: ObjectiveIndex labels the siphoning, and the bar filling
; (QuestTimerGlobalVariable seconds after StageToDisplay) fails the event at StageToSetAtFull.
Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == StageToDisplay
        QuestTimeTotal = 0.0
        SetObjectiveDisplayed(ObjectiveIndex, True)
        StartTimer(ProgressBarUpdateRate, 1)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1 || IsStageDone(9000) || !IsRunning()
        Return
    EndIf
    QuestTimeTotal += ProgressBarUpdateRate
    If QuestTimerGlobalVariable != None && QuestTimeTotal >= QuestTimerGlobalVariable.GetValue()
        If StageToSetAtFull >= 0
            SetStage(StageToSetAtFull)
        EndIf
        Return
    EndIf
    StartTimer(ProgressBarUpdateRate, 1)
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
EndEvent
