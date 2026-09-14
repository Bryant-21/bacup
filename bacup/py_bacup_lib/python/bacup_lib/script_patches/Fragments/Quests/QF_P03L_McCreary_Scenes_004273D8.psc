Function Fragment_Stage_0150_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && CageKey != None
        playerRef.AddItem(CageKey, 1, False)
    EndIf
EndFunction
