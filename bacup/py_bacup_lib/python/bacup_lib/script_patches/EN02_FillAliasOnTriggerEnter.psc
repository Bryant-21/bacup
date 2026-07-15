Event OnTriggerEnter(ObjectReference akActionRef)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || (iShutdownStage > 0 && owningQuest.GetStage() >= iShutdownStage)
        Return
    EndIf
    If AliasToFill != None && AliasToFill.GetRef() == None
        AliasToFill.ForceRefTo(akActionRef)
    EndIf
    If iStageToSet > 0 && !owningQuest.IsStageDone(iStageToSet)
        owningQuest.SetStage(iStageToSet)
    EndIf
EndEvent
