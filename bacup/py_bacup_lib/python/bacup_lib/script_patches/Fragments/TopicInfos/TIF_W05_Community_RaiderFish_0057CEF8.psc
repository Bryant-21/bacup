Function Fragment_Begin(ObjectReference akSpeakerRef)
    If MirelurkEggRef != None && Game.GetPlayer() != None
        Game.GetPlayer().RemoveItem(MirelurkEggRef, 1, True, akSpeakerRef)
    EndIf
EndFunction
