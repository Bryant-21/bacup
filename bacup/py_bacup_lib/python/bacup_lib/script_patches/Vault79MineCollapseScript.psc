Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || myCollapseMarker == None
        Return
    EndIf

    Disable()
    myCollapseMarker.Enable()
    myCollapseMarker.Activate(akActionRef)
EndEvent
