Function Fragment_Begin(ObjectReference akSpeakerRef)
    If MirelurkMeatRef != None && Game.GetPlayer() != None
        Game.GetPlayer().RemoveItem(MirelurkMeatRef, 1, True, akSpeakerRef)
    EndIf
EndFunction
