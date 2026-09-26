Event OnContainerChanged(ObjectReference akNewContainer, ObjectReference akOldContainer)
    If akNewContainer != Game.GetPlayer()
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    ObjectReference droppedItem = GetReference()
    If owningQuest == None || !owningQuest.IsRunning() || droppedItem == None || droppedItem.GetContainer() != akNewContainer
        Return
    EndIf
    If !owningQuest.IsObjectiveCompleted(1610)
        owningQuest.SetObjectiveCompleted(1610)
    EndIf
    Clear()
EndEvent
