Function Fragment_Begin(ObjectReference akSpeakerRef)
    Quest owningQuest = GetOwningQuest()
    If owningQuest && owningQuest.IsStageDone(200) && !owningQuest.IsStageDone(210)
        owningQuest.SetStage(210)
    EndIf
EndFunction
