Event OnQuestInit()
    ActivityCompletionStatus = new Bool[2]
    ActivityCompletionStatus[0] = IsStageDone(DailyTidyStage)
    ActivityCompletionStatus[1] = IsStageDone(DailyStingsStage)
    If IsStageDone(NewTadpoleStage)
        UpdateValueProgress()
    EndIf
EndEvent

Function OnActivityCompleted(Int aiActivityIndex)
    If !IsRunning() || !IsStageDone(NewTadpoleStage) || aiActivityIndex < 0 || aiActivityIndex >= NumActivities
        Return
    EndIf

    If ActivityCompletionStatus == None || ActivityCompletionStatus.Length != NumActivities
        ActivityCompletionStatus = new Bool[2]
        ActivityCompletionStatus[0] = IsStageDone(DailyTidyStage)
        ActivityCompletionStatus[1] = IsStageDone(DailyStingsStage)
    EndIf

    If ActivityCompletionStatus[aiActivityIndex]
        Return
    EndIf

    ActivityCompletionStatus[aiActivityIndex] = true
    If aiActivityIndex == 0
        SetStage(DailyTidyStage)
    ElseIf aiActivityIndex == 1
        SetStage(DailyStingsStage)
    EndIf
    UpdateValueProgress()
EndFunction

Function UpdateValueProgress()
    ValuesCount = 0
    Int valueStage = ValueMinStage
    While valueStage <= ValueMaxStage
        If IsStageDone(valueStage)
            ValuesCount += 1
        EndIf
        valueStage += 100
    EndWhile

    SetObjectiveDisplayed(40, true, true)
    If ValuesCount >= TotalValues && !IsStageDone(ValuesCompletedStage)
        SetStage(ValuesCompletedStage)
    EndIf
EndFunction
