Event OnQuestInit()
    InitializeProgressRun()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If StageToDisplay >= 0 && auiStageID == StageToDisplay
        DisplayProgressBar()
    EndIf
    If StageToHide >= 0 && auiStageID == StageToHide
        HideProgressBar()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 56287
        Return
    EndIf
    If !B21ProgressDisplayed || B21ProgressHidden || !IsRunning()
        Return
    EndIf
    ApplyProgress(CurrPercentage + B21ProgressRate)
    ScheduleProgressTick()
EndEvent

Event OnQuestShutdown()
    CancelTimer(56287)
    B21ProgressDisplayed = False
    B21ProgressInitialized = False
EndEvent

; FO76 quest instances start fresh every run; FO4 keeps script state across
; Stop/Start, so runtime overrides live in variables reset once per run.
Function InitializeProgressRun()
    If B21ProgressInitialized
        Return
    EndIf
    B21ProgressInitialized = True
    B21ProgressDisplayed = False
    B21ProgressHidden = False
    B21ProgressStarted = False
    B21ProgressRate = ModPercentage
    B21ProgressObjective = ObjectiveIndex
    B21ProgressFullStage = StageToSetAtFull
    B21ProgressEmptyStage = StageToSetAtEmpty
    CurrPercentage = 0.0
    B21ProgressFull = False
    B21ProgressEmpty = True
EndFunction

Float Function GetProgress()
    Return CurrPercentage
EndFunction

Function ModProgress(Float percentage)
    InitializeProgressRun()
    If B21ProgressHidden
        Return
    EndIf
    B21ProgressStarted = True
    ApplyProgress(CurrPercentage + percentage)
    ScheduleProgressTick()
EndFunction

Function SetProgress(Float percentage)
    InitializeProgressRun()
    If B21ProgressHidden
        Return
    EndIf
    B21ProgressStarted = True
    ApplyProgress(percentage)
    ScheduleProgressTick()
EndFunction

Function ModChangeRate(Float rate)
    InitializeProgressRun()
    B21ProgressRate += rate
    ScheduleProgressTick()
EndFunction

Function SetChangeRate(Float rate)
    InitializeProgressRun()
    B21ProgressRate = rate
    ScheduleProgressTick()
EndFunction

Function DisplayProgressBar()
    InitializeProgressRun()
    B21ProgressHidden = False
    If !B21ProgressStarted
        B21ProgressStarted = True
        CurrPercentage = ClampProgress(StartPercentage)
        B21ProgressFull = CurrPercentage >= 100.0
        B21ProgressEmpty = CurrPercentage <= 0.0
    EndIf
    B21ProgressDisplayed = True
    ScheduleProgressTick()
EndFunction

Function HideProgressBar()
    InitializeProgressRun()
    CancelTimer(56287)
    B21ProgressDisplayed = False
    B21ProgressHidden = True
EndFunction

Function SetObjectiveIndex(Int index)
    InitializeProgressRun()
    B21ProgressObjective = index
EndFunction

Function SetStageToSetAtFull(Int index)
    InitializeProgressRun()
    B21ProgressFullStage = index
EndFunction

Function SetStageToSetAtEmpty(Int index)
    InitializeProgressRun()
    B21ProgressEmptyStage = index
EndFunction

Float Function ClampProgress(Float percentage)
    If percentage < 0.0
        Return 0.0
    ElseIf percentage > 100.0
        Return 100.0
    EndIf
    Return percentage
EndFunction

Function ApplyProgress(Float percentage)
    CurrPercentage = ClampProgress(percentage)

    Bool isFull = CurrPercentage >= 100.0
    Bool becameFull = isFull && !B21ProgressFull
    B21ProgressFull = isFull

    Bool isEmpty = CurrPercentage <= 0.0
    Bool becameEmpty = isEmpty && !B21ProgressEmpty
    B21ProgressEmpty = isEmpty

    If becameFull
        If B21ProgressFullStage >= 0 && !IsStageDone(B21ProgressFullStage)
            SetStage(B21ProgressFullStage)
        EndIf
        If SendCustomEventAtFull
            SendCustomEvent("ProgressBarFull")
        EndIf
    EndIf
    If becameEmpty
        If B21ProgressEmptyStage >= 0 && !IsStageDone(B21ProgressEmptyStage)
            SetStage(B21ProgressEmptyStage)
        EndIf
        If SendCustomEventAtEmpty
            SendCustomEvent("ProgressBarEmpty")
        EndIf
    EndIf
EndFunction

Function ScheduleProgressTick()
    CancelTimer(56287)
    If !B21ProgressDisplayed || B21ProgressHidden || !(IsRunning() || IsStarting())
        Return
    EndIf
    If B21ProgressRate > 0.0 && CurrPercentage < 100.0
        StartTimer(1.0, 56287)
    ElseIf B21ProgressRate < 0.0 && CurrPercentage > 0.0
        StartTimer(1.0, 56287)
    EndIf
EndFunction
