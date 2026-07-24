Function Fragment_Stage_0010_Item_00()
    If Alias_NukeTapeEnableMarker && Alias_NukeTapeEnableMarker.GetReference()
        Alias_NukeTapeEnableMarker.GetReference().Enable()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If Alias_NukeTapeEnableMarker && Alias_NukeTapeEnableMarker.GetReference()
        Alias_NukeTapeEnableMarker.GetReference().Disable()
    EndIf
EndFunction
