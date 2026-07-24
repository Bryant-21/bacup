Function Fragment_Stage_0100_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && GHL00_Quest_StartKeyword != None
        GHL00_Quest_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    Stop()
EndFunction
