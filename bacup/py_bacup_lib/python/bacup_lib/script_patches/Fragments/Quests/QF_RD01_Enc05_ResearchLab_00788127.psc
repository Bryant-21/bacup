Function Fragment_Stage_0100_Item_00()
    ObjectReference enableMarker = Alias_EnableMarker_Wave01.GetReference()
    If enableMarker
        enableMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    ObjectReference enableMarker = Alias_EnableMarker_Wave01.GetReference()
    If enableMarker
        enableMarker.Disable()
    EndIf
EndFunction
