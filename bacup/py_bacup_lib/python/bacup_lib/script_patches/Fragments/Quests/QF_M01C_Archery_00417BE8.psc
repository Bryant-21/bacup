Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && M01C_Archery_HasFailed != None
        playerRef.SetValue(M01C_Archery_HasFailed, 1.0)
    EndIf
    If M01C_Archery_Failure_Scene != None && !M01C_Archery_Failure_Scene.IsPlaying()
        M01C_Archery_Failure_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && M01C_Archery_HasFailed != None
        playerRef.SetValue(M01C_Archery_HasFailed, 1.0)
    EndIf
    If M01C_Archery_Failure_Scene != None && !M01C_Archery_Failure_Scene.IsPlaying()
        M01C_Archery_Failure_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && M01C_Archery_HasFailed != None
        playerRef.SetValue(M01C_Archery_HasFailed, 2.0)
    EndIf
    If M01C_Archery_Success != None && !M01C_Archery_Success.IsPlaying()
        M01C_Archery_Success.Start()
    EndIf
EndFunction
