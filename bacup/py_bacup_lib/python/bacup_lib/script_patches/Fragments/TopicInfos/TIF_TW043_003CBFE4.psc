Function Fragment_Begin(ObjectReference akSpeakerRef)
    Quest owningQuest = GetOwningQuest()
    TW043QuestScript patrol = owningQuest as TW043QuestScript
    If patrol != None
        patrol.BeginPatrol()
        Return
    EndIf
    If owningQuest != None && owningQuest.IsRunning() && !owningQuest.IsStageDone(10)
        owningQuest.SetStage(10)
    EndIf
EndFunction
