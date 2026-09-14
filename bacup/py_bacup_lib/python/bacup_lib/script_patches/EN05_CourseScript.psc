; FO76 reported course completion to the server, which owned the per-player
; EN05_Basic instance. Single-player substitute: resolve EN05_Basic directly and
; hand it this course's bound iCourseID.
Function EN05Course_ReportCompletion()
    EN05_QuestScript basic = Game.GetFormFromFile(0x0008C87F, "SeventySix.esm") as EN05_QuestScript
    If basic != None
        basic.EN05Basic_CourseCompleted(iCourseID)
    EndIf
EndFunction

Function EN05Course_BeginCountdown()
    fCourseStartTime = 0.0
    fCourseTime = 0.0
    If iCountdownTimerLength > 0
        StartTimer(iCountdownTimerLength as Float, iCourseCountdownTimerID)
    ElseIf !IsStageDone(iCourseStartStage)
        SetStage(iCourseStartStage)
    EndIf
EndFunction

Function EN05Course_StartCourse()
    fCourseStartTime = Utility.GetCurrentRealTime()
EndFunction

Function EN05Course_FinishCourse()
    If fCourseStartTime > 0.0
        fCourseTime = Utility.GetCurrentRealTime() - fCourseStartTime
    EndIf
EndFunction

Function EN05Course_WrapUp(Bool abSuccess)
    Int stageToSet = iCourseFailStage
    If abSuccess || bAlwaysPass
        stageToSet = iCourseSuccessStage
    EndIf
    If !IsStageDone(stageToSet)
        SetStage(stageToSet)
    EndIf
EndFunction

Function EN05Course_ShowResultMessage(Bool abSuccess)
    If !bShowCompletionMessage
        Return
    EndIf
    If abSuccess
        If TrialCompleteMessage != None
            TrialCompleteMessage.Show(fCourseTime)
        EndIf
    ElseIf TrialFailedMessage != None
        TrialFailedMessage.Show(fCourseTime)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == iCourseCountdownTimerID && !IsStageDone(iCourseStartStage)
        SetStage(iCourseStartStage)
    EndIf
EndEvent
