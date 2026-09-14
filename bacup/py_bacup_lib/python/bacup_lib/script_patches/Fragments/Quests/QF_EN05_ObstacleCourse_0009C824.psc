EN05_ObstacleCourseQuestScript Function EN05OBF_GetCourse()
    Return (Self as Quest) as EN05_ObstacleCourseQuestScript
EndFunction

Actor Function EN05OBF_GetPlayer()
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
    EN05_ObstacleCourseQuestScript course = EN05OBF_GetCourse()
    If course != None
        course.EN05OB_BeginCountdown()
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)

    If EN05_ObstacleCourse_OB_Start != None
        EN05_ObstacleCourse_OB_Start.Start()
    EndIf

    If EN05_OB_Start != None && Alias_Loudspeaker != None
        ObjectReference speaker = Alias_Loudspeaker.GetRef()
        If speaker != None
            speaker.Say(EN05_OB_Start)
        EndIf
    EndIf

    EN05_ObstacleCourseQuestScript course = EN05OBF_GetCourse()
    If course != None
        course.EN05OB_StartCourse()
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveDisplayed(20, True, True)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveDisplayed(20, True, True)
EndFunction

Function Fragment_Stage_0099_Item_00()
    EN05_ObstacleCourseQuestScript course = EN05OBF_GetCourse()
    If course != None
        course.EN05OB_AbortCourse()
    EndIf
    SetObjectiveFailed(20)
    If !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(20)
    EN05_ObstacleCourseQuestScript course = EN05OBF_GetCourse()
    If course != None
        course.EN05OB_FinishCourse()
    EndIf
EndFunction

Function Fragment_Stage_0105_Item_00()
    EN05_ObstacleCourseQuestScript course = EN05OBF_GetCourse()
    If course != None
        course.EN05OB_WrapUp(110, 120)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    CompleteAllObjectives()

    Actor playerRef = EN05OBF_GetPlayer()
    If playerRef != None
        If EN05_ObstacleCourseDoneOnceValue != None
            playerRef.SetValue(EN05_ObstacleCourseDoneOnceValue, 1.0)
        EndIf
        If EN05_ObstacleCourseRewardOnceValue != None
            playerRef.SetValue(EN05_ObstacleCourseRewardOnceValue, 1.0)
        EndIf
    EndIf

    EN05_ObstacleCourseQuestScript course = EN05OBF_GetCourse()
    If course != None
        course.EN05OB_ShowResultMessage(True)
        course.EN05Course_ReportCompletion()
    EndIf

    If !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    FailAllObjectives()

    EN05_ObstacleCourseQuestScript course = EN05OBF_GetCourse()
    If course != None
        course.EN05OB_PlayFailure()
        course.EN05OB_ShowResultMessage(False)
    EndIf

    If !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    Stop()
EndFunction
