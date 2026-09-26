Function Fragment_Begin(ObjectReference akSpeakerRef)
    If Alias_Projector != None
        DefaultMultiStateActivator projectorRef = Alias_Projector.GetReference() as DefaultMultiStateActivator
        If projectorRef != None
            projectorRef.SetLocalState(5)
        EndIf
    EndIf
EndFunction

Function Fragment_End(ObjectReference akSpeakerRef)
    If Alias_Projector
        Quest owningQuest = Alias_Projector.GetOwningQuest()
        If owningQuest && !owningQuest.IsStageDone(1400)
            owningQuest.SetStage(1400)
        EndIf
    EndIf
EndFunction
