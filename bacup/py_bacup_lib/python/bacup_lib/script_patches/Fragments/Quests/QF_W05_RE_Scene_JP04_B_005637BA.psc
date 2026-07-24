Function Fragment_Stage_0010_Item_00()
    If SceneRef != None
        SceneRef.Start()
    EndIf
EndFunction

Function Fragment_Stage_0011_Item_00()
EndFunction

Function Fragment_Stage_0013_Item_00()
    If HeadSacRef != None
        ObjectReference baitRef = BaitRef.GetReference()
        If baitRef != None
            baitRef.RemoveItem(HeadSacRef, 1, true)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0015_Item_00()
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction
