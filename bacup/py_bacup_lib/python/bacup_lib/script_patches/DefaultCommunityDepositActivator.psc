Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If RequiredItem == None || RewardItem == None
        Return
    EndIf
    If ConfirmationMessage != None
        If ConfirmationMessage.Show() != 0
            Return
        EndIf
    EndIf
    Actor player = Game.GetPlayer()
    If player.GetItemCount(RequiredItem) < NumRequired
        If NotEnoughItemsMessage != None
            NotEnoughItemsMessage.Show()
        EndIf
        Return
    EndIf
    player.RemoveItem(RequiredItem, NumRequired)
    player.AddItem(RewardItem, 1)
EndEvent
