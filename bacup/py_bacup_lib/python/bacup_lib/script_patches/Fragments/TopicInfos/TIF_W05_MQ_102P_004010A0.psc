Function Fragment_Begin(ObjectReference akSpeakerRef)
    DefaultMultiStateActivator projectorRef = Alias_Projector.GetReference() as DefaultMultiStateActivator
    If projectorRef != None
        projectorRef.SetLocalState(4)
    EndIf
EndFunction
