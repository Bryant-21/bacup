Event OnActivate(ObjectReference akActionRef)
    ObjectReference linkedRef = GetLinkedRef()
    If linkedRef != None
        linkedRef.Unlock()
        If bShouldOpenDoor
            linkedRef.SetOpen(true)
        EndIf
    EndIf
EndEvent
