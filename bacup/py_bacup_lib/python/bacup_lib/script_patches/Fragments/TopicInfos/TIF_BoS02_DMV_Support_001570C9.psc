Function Fragment_End(ObjectReference akSpeakerRef)
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsRunning()
        owningQuest.Stop()
    EndIf
EndFunction
