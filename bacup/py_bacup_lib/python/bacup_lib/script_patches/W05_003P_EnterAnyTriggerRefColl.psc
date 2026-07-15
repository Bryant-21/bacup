Event OnTriggerEnter(ObjectReference akSenderRef, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest == None
        Return
    EndIf
    If ShutDownStage > 0 && owningQuest.IsStageDone(ShutDownStage)
        Return
    EndIf
    If StageToSet > 0 && !owningQuest.IsStageDone(StageToSet)
        owningQuest.SetStage(StageToSet)
    EndIf
EndEvent
