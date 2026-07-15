Event OnActivate(ObjectReference akActionRef)
    ObjectReference linkedRef = GetLinkedRef()
    If linkedRef == None
        Return
    EndIf

    If linkedRef.IsEnabled()
        linkedRef.Disable()
    Else
        linkedRef.Enable()
    EndIf
EndEvent
