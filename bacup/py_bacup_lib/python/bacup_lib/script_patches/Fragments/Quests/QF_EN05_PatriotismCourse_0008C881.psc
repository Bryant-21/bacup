EN05_PatriotismTrainingQuestScript Function EN05PTF_GetCourse()
    Return (Self as Quest) as EN05_PatriotismTrainingQuestScript
EndFunction

Actor Function EN05PTF_GetPlayer()
    Actor playerRef = None
    If Alias_ActivePlayer != None
        playerRef = Alias_ActivePlayer.GetActorReference()
    EndIf
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Return playerRef
EndFunction

Function EN05PTF_GivePlayerItem(Form akItem)
    If akItem == None
        Return
    EndIf
    Actor playerRef = EN05PTF_GetPlayer()
    If playerRef != None && playerRef.GetItemCount(akItem) < 1
        playerRef.AddItem(akItem, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    EN05_PatriotismTrainingQuestScript course = EN05PTF_GetCourse()
    If course != None
        course.EN05PT_BeginCourse()
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveDisplayed(20)
    SetObjectiveDisplayed(21)
    SetObjectiveDisplayed(22)

    If EN05_Patriotism_Start != None && Alias_Loudspeaker != None
        ObjectReference speaker = Alias_Loudspeaker.GetRef()
        If speaker != None
            speaker.Say(EN05_Patriotism_Start)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(21)
    SetObjectiveDisplayed(30)
    If IsStageDone(40) && !IsStageDone(50)
        SetStage(50)
    EndIf
EndFunction

Function Fragment_Stage_0040_Item_00()
    Actor playerRef = EN05PTF_GetPlayer()
    If playerRef != None && EN05_PlayerReadJimmyDiaryValue != None
        playerRef.SetValue(EN05_PlayerReadJimmyDiaryValue, 1.0)
    EndIf
    SetObjectiveCompleted(30)
    If IsStageDone(30) && !IsStageDone(50)
        SetStage(50)
    EndIf
EndFunction

Function Fragment_Stage_0041_Item_00()
    EN05_PatriotismTrainingQuestScript course = EN05PTF_GetCourse()
    If course != None
        course.EN05PT_HandleDiaryRead(EN05_Patriotism_JimmyDiary)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(30)
    EN05PTF_GivePlayerItem(EN05_Patriotism_JimmyTerminalPassword)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(50)
    EN05PTF_GivePlayerItem(EN05_Patriotism_JimmyEvidence)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0070_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0080_Item_00()
    Actor playerRef = EN05PTF_GetPlayer()
    If playerRef != None && EN05_PlayerReadTopherDiaryValue != None
        playerRef.SetValue(EN05_PlayerReadTopherDiaryValue, 1.0)
    EndIf
    SetObjectiveCompleted(70)
EndFunction

Function Fragment_Stage_0081_Item_00()
    EN05_PatriotismTrainingQuestScript course = EN05PTF_GetCourse()
    If course != None
        course.EN05PT_HandleDiaryRead(EN05_Patriotism_TophersDiary)
    EndIf
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveCompleted(22)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(60)

    Actor playerRef = EN05PTF_GetPlayer()
    If playerRef != None && EN05_Patriotism_PlayerPassedValue != None
        playerRef.SetValue(EN05_Patriotism_PlayerPassedValue, 1.0)
    EndIf

    EN05_PatriotismTrainingQuestScript course = EN05PTF_GetCourse()
    If course != None
        course.EN05PT_FinishCourse()
    EndIf
EndFunction

Function Fragment_Stage_0105_Item_00()
    EN05_PatriotismTrainingQuestScript course = EN05PTF_GetCourse()
    If course != None
        course.EN05PT_WrapUp(110, 120)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    CompleteAllObjectives()

    Actor playerRef = EN05PTF_GetPlayer()
    If playerRef != None
        If EN05_PatriotismCourseDoneOnceValue != None
            playerRef.SetValue(EN05_PatriotismCourseDoneOnceValue, 1.0)
        EndIf
        If EN05_PatriotismCourseRewardOnceValue != None
            playerRef.SetValue(EN05_PatriotismCourseRewardOnceValue, 1.0)
        EndIf
    EndIf

    EN05_PatriotismTrainingQuestScript course = EN05PTF_GetCourse()
    If course != None
        course.EN05Course_ReportCompletion()
    EndIf

    If !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    FailAllObjectives()
    If !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    Stop()
EndFunction
