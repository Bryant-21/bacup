Function Fragment_Stage_0010_Item_00()
    If SandboxScene != None
        SandboxScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0019_Item_00()
    If DialogueScene != None
        DialogueScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    If CaptiveSitScene != None
        CaptiveSitScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
EndFunction

Function Fragment_Stage_0040_Item_00()
    If HeadSacRef != None
        ObjectReference captive01 = CaptiveRef01.GetReference()
        If captive01 != None
            captive01.RemoveItem(HeadSacRef, 1, true)
        EndIf
        ObjectReference captive02 = CaptiveRef02.GetReference()
        If captive02 != None
            captive02.RemoveItem(HeadSacRef, 1, true)
        EndIf
        ObjectReference captive03 = CaptiveRef03.GetReference()
        If captive03 != None
            captive03.RemoveItem(HeadSacRef, 1, true)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction
