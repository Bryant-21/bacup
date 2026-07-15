Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    SetOpen(shouldOpen)
    ObjectReference linkedRef = GetLinkedRef(LinkedRefKeyword)
    If linkedRef != None
        linkedRef.SetOpen(shouldOpen)
    EndIf
EndEvent
