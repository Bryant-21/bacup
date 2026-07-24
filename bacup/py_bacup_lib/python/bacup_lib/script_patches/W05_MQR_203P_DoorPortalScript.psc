Event OnActivate(ObjectReference akActionRef)
    If akActionRef == None || Alias_Destination == None
        Return
    EndIf

    ObjectReference destinationRef = Alias_Destination.GetReference()
    If destinationRef != None
        akActionRef.MoveTo(destinationRef)
    EndIf
EndEvent
