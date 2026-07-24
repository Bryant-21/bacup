Function Fragment_End(ObjectReference akSpeakerRef)
    If RemoveRef != None
        Game.GetPlayer().RemoveItem(RemoveRef, 1)
    EndIf
EndFunction
