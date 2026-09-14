Event OnActivate(ObjectReference akActionRef)
    If bBusy || akActionRef != Game.GetPlayer() || ItemToRemove == None
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest == None
        Return
    EndIf
    If PrereqStage >= 0 && !owningQuest.IsStageDone(PrereqStage)
        Return
    EndIf
    If TurnOffStage >= 0 && owningQuest.GetCurrentStageID() >= TurnOffStage
        Return
    EndIf

    Int requiredCount = iItemCountRequired
    If requiredCount < 1
        requiredCount = 1
    EndIf
    If akActionRef.GetItemCount(ItemToRemove) < requiredCount
        If MessageInsufficientItems != None
            MessageInsufficientItems.Show()
        EndIf
        Return
    EndIf

    bBusy = True
    akActionRef.RemoveItem(ItemToRemove, requiredCount, True)
    parent.OnActivate(akActionRef)
    bBusy = False
EndEvent
