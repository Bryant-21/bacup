Function Fragment_Stage_0010_Item_00()
    If Alias_ClutterMarkerEnable.GetReference() != None
        Alias_ClutterMarkerEnable.GetReference().Enable()
    EndIf
    If Alias_ClutterMarkerDisable.GetReference() != None
        Alias_ClutterMarkerDisable.GetReference().Disable()
    EndIf
EndFunction

Function Fragment_Stage_0011_Item_00()
    If StartDialogue != None
        StartDialogue.Start()
    EndIf
    If SceneRef != None
        SceneRef.Start()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction
