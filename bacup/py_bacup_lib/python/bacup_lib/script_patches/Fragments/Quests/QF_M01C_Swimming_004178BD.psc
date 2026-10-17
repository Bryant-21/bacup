Function Fragment_Stage_0100_Item_00()
    If Scene_Start != None && !Scene_Start.IsPlaying()
        Scene_Start.Start()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_HasFailed != None
        playerRef.SetValue(AV_HasFailed, 1.0)
    EndIf
    If Scene_Failure != None && !Scene_Failure.IsPlaying()
        Scene_Failure.Start()
    EndIf
EndFunction

Function Fragment_Stage_0998_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_HasFailed != None
        playerRef.SetValue(AV_HasFailed, 1.0)
    EndIf
    If Scene_Failure != None && !Scene_Failure.IsPlaying()
        Scene_Failure.Start()
    EndIf
    FailAllObjectives()
EndFunction

Function Fragment_Stage_0999_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_HasFailed != None
        playerRef.SetValue(AV_HasFailed, 1.0)
    EndIf
    If Scene_Failure != None && !Scene_Failure.IsPlaying()
        Scene_Failure.Start()
    EndIf
    FailAllObjectives()
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    ; No Stop() here: stage 400 starts M01C_Swimming_Results after setting 9000, and that scene's
    ; completion sets 9001, which stops the quest (Swimming_QuestScript).
EndFunction
