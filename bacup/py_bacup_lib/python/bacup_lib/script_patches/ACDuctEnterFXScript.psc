Event OnActivate(ObjectReference akActionRef)
    If LinkedRefKeyword != None
        ObjectReference target = GetLinkedRef(LinkedRefKeyword)
        If target != None
            target.Activate(akActionRef)
        EndIf
    EndIf
EndEvent
