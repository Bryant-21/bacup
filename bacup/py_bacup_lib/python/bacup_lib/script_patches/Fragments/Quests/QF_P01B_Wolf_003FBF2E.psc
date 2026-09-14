Function Fragment_Stage_0300_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && P01B_Wolf_RecallKey != None
        playerRef.AddItem(P01B_Wolf_RecallKey, 1, False)
    EndIf
EndFunction
