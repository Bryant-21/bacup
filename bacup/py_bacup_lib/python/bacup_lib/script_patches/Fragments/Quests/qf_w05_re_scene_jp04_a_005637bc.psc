Function Fragment_Stage_0010_Item_00()
    If RunSceneRef != None
        RunSceneRef.Start()
    EndIf
EndFunction

Function Fragment_Stage_0011_Item_00()
    If RealSceneProp != None
        RealSceneProp.Start()
    EndIf
EndFunction

Function Fragment_Stage_0019_Item_00()
EndFunction

Function Fragment_Stage_0020_Item_00()
    If HeadSacRef != None
        ObjectReference settlerRef = SettlerRef.GetReference()
        If settlerRef != None
            settlerRef.RemoveItem(HeadSacRef, 1, true)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction
