Function Fragment_End(ObjectReference akSpeakerRef)
    If Alias_Projector
        Quest owningQuest = Alias_Projector.GetOwningQuest()
        If owningQuest && !owningQuest.IsStageDone(1400)
            owningQuest.SetStage(1400)
        EndIf
    EndIf
EndFunction
