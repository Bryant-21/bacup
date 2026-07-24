Event OnActivate(ObjectReference akActionRef)
    If PlayerOnly && akActionRef != Game.GetPlayer()
        Return
    EndIf
    If activationLock
        Return
    EndIf
    If DoNotAddDuplicate && akActionRef.GetItemCount(ItemToAdd) > 0
        Return
    EndIf

    Int qty = ItemQuantity
    If qty <= 0
        qty = 1
    EndIf
    akActionRef.AddItem(ItemToAdd, qty)

    If BlockFutureActivation
        activationLock = True
    EndIf
EndEvent
