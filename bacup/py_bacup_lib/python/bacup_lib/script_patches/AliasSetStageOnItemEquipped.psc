Event OnItemEquipped(Form akBaseObject, ObjectReference akReference)
    If akBaseObject == ItemToCheck
        Quest owningQuest = GetOwningQuest()
        If owningQuest && (iPrereqStage == -1 || owningQuest.IsStageDone(iPrereqStage))
            owningQuest.SetStage(iStageToSet)
        EndIf
    EndIf
EndEvent
