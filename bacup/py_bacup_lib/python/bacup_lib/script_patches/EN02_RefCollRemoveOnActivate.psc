Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    RemoveRef(akSenderRef)
    Quest owningQuest = GetOwningQuest()
    If iStageToSet > 0 && owningQuest != None && !owningQuest.IsStageDone(iStageToSet)
        owningQuest.SetStage(iStageToSet)
    EndIf
EndEvent
