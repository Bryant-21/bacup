Event OnActivate(ObjectReference akActionRef)
    ObjectReference target = GetLinkedRef(LinkedRefKeyword)
    If target != None
        target.Activate(akActionRef)
    EndIf
EndEvent
