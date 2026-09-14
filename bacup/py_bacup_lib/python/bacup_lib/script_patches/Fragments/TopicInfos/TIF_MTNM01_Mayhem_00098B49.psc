Function Fragment_Begin(ObjectReference akSpeakerRef)
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsRunning() && !owningQuest.IsStageDone(400)
        owningQuest.SetStage(400)
    EndIf
EndFunction
