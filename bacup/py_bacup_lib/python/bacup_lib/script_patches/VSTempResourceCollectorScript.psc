Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If TempResourceCollectorMessage == None
        Return
    EndIf

    Int button = TempResourceCollectorMessage.Show(GetItemCount(ResourceToCollect) as Float)
    If button == 0
        RemoveItem(ResourceToCollect, GetItemCount(ResourceToCollect), false, akActionRef)
    EndIf
EndEvent
