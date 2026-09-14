; FO76 reported course completion to the server, which owned the per-player
; EN05_Basic instance. Single-player substitute: resolve EN05_Basic directly and
; hand it this course's bound iCourseID.
Function EN05Course_ReportCompletion()
    EN05_QuestScript basic = Game.GetFormFromFile(0x0008C87F, "SeventySix.esm") as EN05_QuestScript
    If basic != None
        basic.EN05Basic_CourseCompleted(iCourseID)
    EndIf
EndFunction

Function EN05MC_BeginCountdown()
    fCourseStartTime = 0.0
    fCourseTime = 0.0
    bFinalCourseSuccess = False
    fFinalCourseTime = 0.0
    fRouteTime = 0.0
    If iCountdownTimerLength > 0
        StartTimer(iCountdownTimerLength as Float, iCourseCountdownTimerID)
    ElseIf !IsStageDone(iCourseStartStage)
        SetStage(iCourseStartStage)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == iCourseCountdownTimerID && !IsStageDone(iCourseStartStage)
        SetStage(iCourseStartStage)
    EndIf
EndEvent

Function EN05MC_StartCourse()
    fCourseStartTime = Utility.GetCurrentRealTime()

    ; The collection may not have been populated when its alias initialised, so
    ; arm the hit registration again once the course is actually live.
    EN05_TargetAliasCollectionScript targetSet = Targets as EN05_TargetAliasCollectionScript
    If targetSet != None
        targetSet.EN05MCT_ArmTargets()
    EndIf

    If EN05_MarksmanshipTraining_0020_Start != None
        EN05_MarksmanshipTraining_0020_Start.Start()
    EndIf
EndFunction

Function EN05MC_FinishCourse()
    If fCourseStartTime > 0.0
        fCourseTime = Utility.GetCurrentRealTime() - fCourseStartTime
    EndIf
    fFinalCourseTime = fCourseTime
    fRouteTime = fCourseTime
    bFinalCourseSuccess = True
    If EN05_MarksmanshipTraining_0110_Success != None
        EN05_MarksmanshipTraining_0110_Success.Start()
    EndIf
EndFunction

Function EN05MC_WrapUp(Int aiSuccessStage, Int aiFailStage)
    Int stageToSet = aiFailStage
    If bFinalCourseSuccess
        stageToSet = aiSuccessStage
    EndIf
    If !IsStageDone(stageToSet)
        SetStage(stageToSet)
    EndIf
EndFunction

Function EN05MC_PlayFailure()
    If EN05_MarksmanshipTraining_0120_Failure != None
        EN05_MarksmanshipTraining_0120_Failure.Start()
    EndIf
EndFunction

Function EN05MC_ShowResultMessage(Bool abSuccess)
    If abSuccess
        If TrialCompleteMessage != None
            TrialCompleteMessage.Show(fCourseTime)
        EndIf
    ElseIf TrialFailedMessage != None
        TrialFailedMessage.Show(fCourseTime)
    EndIf
EndFunction
