; Children such as Quests:E05_Radiation:OnActivateScript override this hook.
Function OnProgressItemsDeposited(Actor akActivator, Int aiCount)
EndFunction

Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef == None || akActionRef != playerRef || !CanContribute()
        Return
    EndIf

    Int contributed = 1
    If ItemToRemove != None
        Int heldCount = playerRef.GetItemCount(ItemToRemove)
        Int requiredCount = NumToRemove
        If requiredCount < 1
            requiredCount = 1
        EndIf
        If heldCount < requiredCount
            PlayErrorFeedback()
            Return
        EndIf
        Int removeCount = NumToRemove
        If NumToRemove < 0
            removeCount = heldCount
        EndIf
        playerRef.RemoveItem(ItemToRemove, removeCount)
        contributed = heldCount - playerRef.GetItemCount(ItemToRemove)
        If contributed <= 0
            PlayErrorFeedback()
            Return
        EndIf
        If NumToRemove > 0
            ContributeProgress(contributed as Float / NumToRemove as Float)
        Else
            ContributeProgress(contributed as Float)
        EndIf
    Else
        ContributeProgress(1.0)
    EndIf
    OnProgressItemsDeposited(playerRef, contributed)
EndEvent
