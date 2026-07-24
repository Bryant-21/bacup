Event OnGetUp(ObjectReference akFurniture)
    Quest owningQuest = GetOwningQuest()
    If owningQuest && owningQuest.IsStageDone(PreReqStage)
        owningQuest.SetStage(StageToSet)
    EndIf
EndEvent
