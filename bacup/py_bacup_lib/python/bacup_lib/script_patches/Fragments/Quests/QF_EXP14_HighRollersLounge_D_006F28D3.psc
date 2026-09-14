Function Fragment_Stage_0100_Item_00()
    ObjectReference doorRef = Alias_MainDoor.GetReference()
    If doorRef != None
        doorRef.Unlock()
    EndIf
EndFunction
