; FO76 reported course completion to the server, which owned the per-player
; EN05_Basic instance. Single-player substitute: resolve EN05_Basic directly and
; hand it this course's bound iCourseID.
Function EN05Course_ReportCompletion()
    EN05_QuestScript basic = Game.GetFormFromFile(0x0008C87F, "SeventySix.esm") as EN05_QuestScript
    If basic != None
        basic.EN05Basic_CourseCompleted(iCourseID)
    EndIf
EndFunction

Function EN05OB_BeginCountdown()
    fCourseStartTime = 0.0
    fCourseTime = 0.0
    fRouteTime = 0.0
    fFinalCourseTime = 0.0
    bFinalCourseSuccess = False
    iNextTargetCollection = 0
    If EN05_ObstacleCourse_OB_Intro != None
        EN05_ObstacleCourse_OB_Intro.Start()
    EndIf
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

Function EN05OB_ActivateTargetSet(Int aiIndex)
    If TargetData == None || aiIndex < 0 || aiIndex >= TargetData.Length
        Return
    EndIf

    iNextTargetCollection = aiIndex
    RefCollectionAlias nextSet = TargetData[aiIndex].TargetCollection
    If ActiveTargets == None || nextSet == None
        Return
    EndIf

    ActiveTargets.RemoveAll()
    ActiveTargets.AddRefCollection(nextSet)

    EN05_ObstacleAliasCollectionScript setScript = nextSet as EN05_ObstacleAliasCollectionScript
    If setScript != None
        setScript.bObstaclesActive = True
    EndIf

    If TargetData[aiIndex].StartUpStage > 0 && !IsStageDone(TargetData[aiIndex].StartUpStage)
        SetStage(TargetData[aiIndex].StartUpStage)
    EndIf
EndFunction

Function EN05OB_StartCourse()
    fCourseStartTime = Utility.GetCurrentRealTime()
    EN05OB_ActivateTargetSet(0)
EndFunction

Function EN05OB_AdvanceTargetSet()
    Int nextIndex = iNextTargetCollection + 1
    If TargetData == None || nextIndex >= TargetData.Length
        If !IsStageDone(iCourseWrapupStage)
            SetStage(iCourseWrapupStage)
        EndIf
        Return
    EndIf

    If nextIndex == TargetData.Length - 1
        If EN05_ObstacleCourse_OB_0035_LastObstacle != None
            EN05_ObstacleCourse_OB_0035_LastObstacle.Start()
        EndIf
    ElseIf EN05_ObstacleCourse_OB_0030_ObstacleCleared != None
        EN05_ObstacleCourse_OB_0030_ObstacleCleared.Start()
    EndIf

    EN05OB_ActivateTargetSet(nextIndex)
EndFunction

Function EN05OB_ClearActiveTargets()
    If ActiveTargets != None
        ActiveTargets.RemoveAll()
    EndIf
    Int index = 0
    While TargetData != None && index < TargetData.Length
        EN05_ObstacleAliasCollectionScript setScript = TargetData[index].TargetCollection as EN05_ObstacleAliasCollectionScript
        If setScript != None
            setScript.bObstaclesActive = False
        EndIf
        index += 1
    EndWhile
EndFunction

Function EN05OB_FinishCourse()
    If fCourseStartTime > 0.0
        fCourseTime = Utility.GetCurrentRealTime() - fCourseStartTime
    EndIf
    fFinalCourseTime = fCourseTime
    bFinalCourseSuccess = True
    EN05OB_ClearActiveTargets()
    If EN05_ObstacleCourse_OB_Success != None
        EN05_ObstacleCourse_OB_Success.Start()
    EndIf
EndFunction

Function EN05OB_AbortCourse()
    CancelTimer(iCourseCountdownTimerID)
    EN05OB_ClearActiveTargets()
    If EN05_ObstacleCourse_OB_Restart != None
        EN05_ObstacleCourse_OB_Restart.Start()
    EndIf
EndFunction

Function EN05OB_PlayFailure()
    EN05OB_ClearActiveTargets()
    If EN05_ObstacleCourse_OB_Failure != None
        EN05_ObstacleCourse_OB_Failure.Start()
    EndIf
EndFunction

Function EN05OB_WrapUp(Int aiSuccessStage, Int aiFailStage)
    Int stageToSet = aiFailStage
    If bFinalCourseSuccess
        stageToSet = aiSuccessStage
    EndIf
    If !IsStageDone(stageToSet)
        SetStage(stageToSet)
    EndIf
EndFunction

; The complete message reads "%.0f:%.0f" and the _Zeroed variant "%.0f:0%.0f",
; so the zeroed form is the one that supplies the leading zero for a
; single-digit seconds remainder.
Function EN05OB_ShowResultMessage(Bool abSuccess)
    Int totalSeconds = fCourseTime as Int
    Int minutes = totalSeconds / 60
    Int seconds = totalSeconds - minutes * 60

    If !abSuccess
        If TrialFailedMessage != None
            TrialFailedMessage.Show(fCourseTime, fRouteTime)
        EndIf
        Return
    EndIf

    If seconds <= MinNumSeconds
        If EN05_ObstacleCourseCompleteMessage_Zeroed != None
            EN05_ObstacleCourseCompleteMessage_Zeroed.Show(minutes as Float, seconds as Float)
        EndIf
    ElseIf TrialCompleteMessage != None
        TrialCompleteMessage.Show(minutes as Float, seconds as Float)
    EndIf
EndFunction
