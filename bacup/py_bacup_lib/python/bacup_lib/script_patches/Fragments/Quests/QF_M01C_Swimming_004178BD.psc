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
EndFunction

Function Fragment_Stage_0999_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_HasFailed != None
        playerRef.SetValue(AV_HasFailed, 1.0)
    EndIf
    If Scene_Failure != None && !Scene_Failure.IsPlaying()
        Scene_Failure.Start()
    EndIf
EndFunction
