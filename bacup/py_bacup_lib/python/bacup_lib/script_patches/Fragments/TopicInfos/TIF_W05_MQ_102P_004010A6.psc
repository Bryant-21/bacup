Function Fragment_Begin(ObjectReference akSpeakerRef)
    DefaultMultiStateActivator projectorRef = Projector.GetReference() as DefaultMultiStateActivator
    If projectorRef != None
        projectorRef.SetLocalState(3)
    EndIf
EndFunction
