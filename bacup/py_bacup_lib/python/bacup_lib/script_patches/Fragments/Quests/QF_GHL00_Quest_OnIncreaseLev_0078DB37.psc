Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && GHL00_Quest_StartKeyword != None
        GHL00_Quest_StartKeyword.SendStoryEventAndWait(None, playerRef)
    EndIf
    Stop()
EndFunction
