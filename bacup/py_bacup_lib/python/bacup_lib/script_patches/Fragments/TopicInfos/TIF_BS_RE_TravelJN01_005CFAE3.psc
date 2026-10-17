Function Fragment_Begin(ObjectReference akSpeakerRef)
    Quest owningQuest = GetOwningQuest()
    If owningQuest && !owningQuest.IsStageDone(100)
        owningQuest.SetStage(100)
    EndIf
EndFunction
