Function Fragment_Stage_0100_Item_00()
    If Scene_Start && !Scene_Start.IsPlaying()
        Scene_Start.Start()
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    ObjectReference currentInstructor = Alias_CURRENT_Instructor.GetReference()
    If currentInstructor == None
        Quests:M01C:Athletics_QuestScript athleticsController = (Self as Quest) as Quests:M01C:Athletics_QuestScript
        If athleticsController != None
            currentInstructor = athleticsController.SelectCurrentInstructor()
        EndIf
    EndIf
    If currentInstructor == Alias_Venture_Instructor.GetReference()
        SetStage(200)
    ElseIf currentInstructor == Alias_Bridge_Instructor.GetReference()
        SetStage(300)
    ElseIf currentInstructor == Alias_Slopes_Instructor.GetReference()
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    Keyword ventureStart = Game.GetFormFromFile(0x004270F9, "SeventySix.esm") as Keyword
    If playerRef != None && ventureStart != None
        ventureStart.SendStoryEventAndWait(None, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && StartKeyword_M01C_Athletics_Bridge != None
        StartKeyword_M01C_Athletics_Bridge.SendStoryEventAndWait(None, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    Keyword slopesStart = Game.GetFormFromFile(0x004270FA, "SeventySix.esm") as Keyword
    If playerRef != None && slopesStart != None
        slopesStart.SendStoryEventAndWait(None, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    If Scene_Fail && !Scene_Fail.IsPlaying()
        Scene_Fail.Start()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    If Scene_Success && !Scene_Success.IsPlaying()
        Scene_Success.Start()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetStage(9001)
EndFunction

Function Fragment_Stage_9001_Item_00()
    Stop()
EndFunction
