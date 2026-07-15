Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || W05_RE_MapMasterDummyMarker_Keyword == None
        Return
    EndIf

    ObjectReference[] markers = GetLinkedRefChildren(W05_RE_MapMasterDummyMarker_Keyword)
    Int markerIndex = 0
    While markerIndex < markers.Length
        If markers[markerIndex].IsEnabled()
            markers[markerIndex].Disable()
        Else
            markers[markerIndex].Enable()
        EndIf
        markerIndex += 1
    EndWhile

    If LightToggleSound != None
        LightToggleSound.Play(Self)
    EndIf
EndEvent
