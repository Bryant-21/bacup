Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest == None
        Return
    EndIf
    If PreReqStage > 0 && !owningQuest.IsStageDone(PreReqStage)
        Return
    EndIf
    If StageToSet > 0 && !owningQuest.IsStageDone(StageToSet)
        owningQuest.SetStage(StageToSet)
    EndIf
EndEvent
