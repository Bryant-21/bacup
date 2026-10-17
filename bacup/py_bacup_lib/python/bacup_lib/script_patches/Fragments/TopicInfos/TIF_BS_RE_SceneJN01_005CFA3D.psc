Function Fragment_Begin(ObjectReference akSpeakerRef)
    Quest owningQuest = GetOwningQuest()
    If owningQuest && !owningQuest.IsStageDone(120)
        owningQuest.SetStage(120)
    EndIf
EndFunction
