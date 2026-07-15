Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        akActionRef.AddItem(BookToPrint, 1, False)
    EndIf
EndEvent
