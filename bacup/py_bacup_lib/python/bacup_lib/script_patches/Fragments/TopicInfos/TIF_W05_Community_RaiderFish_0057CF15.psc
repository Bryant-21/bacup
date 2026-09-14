Function Fragment_Begin(ObjectReference akSpeakerRef)
    If MirelurkQueenMeatRef != None && Game.GetPlayer() != None
        Game.GetPlayer().RemoveItem(MirelurkQueenMeatRef, 1, True, akSpeakerRef)
    EndIf
EndFunction
