Event OnContainerChanged(ObjectReference akNewContainer, ObjectReference akOldContainer)
    If akNewContainer != Game.GetPlayer()
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && !owningQuest.IsObjectiveCompleted(1610)
        owningQuest.SetObjectiveCompleted(1610)
    EndIf
EndEvent
