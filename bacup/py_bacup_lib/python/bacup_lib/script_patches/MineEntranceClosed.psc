Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    CB_MineClosed_TempMsg.Show()
    SendStoryEventKeyword.SendStoryEvent(GetCurrentLocation(), akActionRef, GetLinkedRef(LinkMine))
EndEvent
