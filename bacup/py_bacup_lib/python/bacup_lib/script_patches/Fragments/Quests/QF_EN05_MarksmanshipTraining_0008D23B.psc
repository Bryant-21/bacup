EN05_MarksmanshipCourseScript Function EN05MCF_GetCourse()
    Return (Self as Quest) as EN05_MarksmanshipCourseScript
EndFunction

Actor Function EN05MCF_GetPlayer()
    Actor playerRef = None
    If Alias_ActivePlayer != None
        playerRef = Alias_ActivePlayer.GetActorReference()
    EndIf
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Return playerRef
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    EN05_MarksmanshipCourseScript course = EN05MCF_GetCourse()
    If course != None
        course.EN05MC_BeginCountdown()
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)

    EN05_MarksmanshipCourseScript course = EN05MCF_GetCourse()
    If course != None
        course.EN05MC_StartCourse()
    EndIf

    If EN05_MC_Start != None && Alias_Loudspeaker != None
        ObjectReference speaker = Alias_Loudspeaker.GetRef()
        If speaker != None
            speaker.Say(EN05_MC_Start)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(20)
    EN05_MarksmanshipCourseScript course = EN05MCF_GetCourse()
    If course != None
        course.EN05MC_FinishCourse()
    EndIf
EndFunction

Function Fragment_Stage_0105_Item_00()
    EN05_MarksmanshipCourseScript course = EN05MCF_GetCourse()
    If course != None
        course.EN05MC_WrapUp(110, 120)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    CompleteAllObjectives()

    Actor playerRef = EN05MCF_GetPlayer()
    If playerRef != None
        If EN05_MarksmanshipCourseDoneOnceValue != None
            playerRef.SetValue(EN05_MarksmanshipCourseDoneOnceValue, 1.0)
        EndIf
        If EN05_MarksmanshipCourseRewardOnceValue != None
            playerRef.SetValue(EN05_MarksmanshipCourseRewardOnceValue, 1.0)
        EndIf
    EndIf

    EN05_MarksmanshipCourseScript course = EN05MCF_GetCourse()
    If course != None
        course.EN05MC_ShowResultMessage(True)
        course.EN05Course_ReportCompletion()
    EndIf

    If !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    FailAllObjectives()

    EN05_MarksmanshipCourseScript course = EN05MCF_GetCourse()
    If course != None
        course.EN05MC_PlayFailure()
        course.EN05MC_ShowResultMessage(False)
    EndIf

    If !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    Stop()
EndFunction
