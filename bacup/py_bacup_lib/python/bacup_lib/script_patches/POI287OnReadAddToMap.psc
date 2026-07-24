Event OnRead()
    If MapMarker01 != None
        MapMarker01.AddToMap(ShouldMarkDiscovered)
    EndIf
    If MapMarker02 != None
        MapMarker02.AddToMap(ShouldMarkDiscovered)
    EndIf
EndEvent
