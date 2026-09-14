; FO76 reported course completion to the server, which owned the per-player
; EN05_Basic instance. Single-player substitute: resolve EN05_Basic directly and
; hand it this course's bound iCourseID.
Function EN05Course_ReportCompletion()
    EN05_QuestScript basic = Game.GetFormFromFile(0x0008C87F, "SeventySix.esm") as EN05_QuestScript
    If basic != None
        basic.EN05Basic_CourseCompleted(iCourseID)
    EndIf
EndFunction

Actor Function EN05PT_GetPlayer()
    Actor playerRef = None
    If ActivePlayer != None
        playerRef = ActivePlayer.GetActorReference()
    EndIf
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Return playerRef
EndFunction

; This course has no countdown properties, so the start-up stage hands straight
; over to the course start stage.
Function EN05PT_BeginCourse()
    fCourseStartTime = Utility.GetCurrentRealTime()
    fCourseTime = 0.0
    If !IsStageDone(iCourseStartStage)
        SetStage(iCourseStartStage)
    EndIf
EndFunction

Function EN05PT_HandleDiaryRead(Book akDiary)
    If akDiary == None || DiaryData == None
        Return
    EndIf

    Actor playerRef = EN05PT_GetPlayer()
    Int index = 0
    While index < DiaryData.Length
        If DiaryData[index].DiaryNote == akDiary
            If playerRef != None && DiaryData[index].ReadValue != None
                playerRef.SetValue(DiaryData[index].ReadValue, 1.0)
            EndIf
            If DiaryData[index].iStageToSetOnRead > 0 && !IsStageDone(DiaryData[index].iStageToSetOnRead)
                SetStage(DiaryData[index].iStageToSetOnRead)
            EndIf
            Return
        EndIf
        index += 1
    EndWhile
EndFunction

Function EN05PT_FinishCourse()
    If fCourseStartTime > 0.0
        fCourseTime = Utility.GetCurrentRealTime() - fCourseStartTime
    EndIf
    If EN05_PatriotismCourse_0110_Success != None
        EN05_PatriotismCourse_0110_Success.Start()
    EndIf
EndFunction

Function EN05PT_WrapUp(Int aiSuccessStage, Int aiFailStage)
    Int stageToSet = aiFailStage
    If IsStageDone(iCourseWrapupStage)
        stageToSet = aiSuccessStage
    EndIf
    If !IsStageDone(stageToSet)
        SetStage(stageToSet)
    EndIf
EndFunction
