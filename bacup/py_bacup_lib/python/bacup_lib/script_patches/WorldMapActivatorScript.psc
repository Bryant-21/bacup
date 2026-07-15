Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && MapMarker != None
        MapMarker.AddToMap(False)
    EndIf
EndEvent
