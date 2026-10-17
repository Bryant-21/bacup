Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akSenderRef == None || akActionRef != playerRef || !CanContribute()
        Return
    EndIf

    Float contributions = 1.0
    If ItemToRemove != None
        Int heldCount = playerRef.GetItemCount(ItemToRemove)
        Int requiredCount = NumToRemove
        If requiredCount < 1
            requiredCount = 1
        EndIf
        If heldCount < requiredCount
            PlayErrorFeedback(akSenderRef)
            Return
        EndIf
        Int removeCount = NumToRemove
        If NumToRemove < 0
            removeCount = heldCount
        EndIf
        playerRef.RemoveItem(ItemToRemove, removeCount)
        Int removed = heldCount - playerRef.GetItemCount(ItemToRemove)
        If removed <= 0
            PlayErrorFeedback(akSenderRef)
            Return
        EndIf
        If NumToRemove > 0
            contributions = removed as Float / NumToRemove as Float
        Else
            contributions = removed as Float
        EndIf
    EndIf
    ContributeProgress(contributions, akSenderRef)

    If KeywordToAddOnActivate != None
        akSenderRef.AddKeyword(KeywordToAddOnActivate)
    EndIf
    If EnableLinkedRefOnActivate && akSenderRef.GetLinkedRef() != None
        akSenderRef.GetLinkedRef().Enable()
    EndIf
    If BlockActivationOnActivate
        akSenderRef.BlockActivation(True, True)
    EndIf
    NumActivated += 1
    If RemoveFromAliasOnActivate
        RemoveRef(akSenderRef)
    EndIf
    If DeleteRefOnActivate
        akSenderRef.Disable()
        akSenderRef.Delete()
    ElseIf DisableRefOnActivate
        akSenderRef.Disable()
    EndIf
    Quest owningQuest = GetOwningQuest()
    If ObjectiveToComplete >= 0 && owningQuest.IsObjectiveDisplayed(ObjectiveToComplete) && (GetCount() == 0 || NumActivated >= GetCount())
        owningQuest.SetObjectiveCompleted(ObjectiveToComplete, True)
    EndIf
EndEvent
