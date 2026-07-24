Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If MapMarkerToUnlock != None && !MapMarkerToUnlock.IsMapMarkerVisible()
        MapMarkerToUnlock.AddToMap(False)
    EndIf
EndEvent
