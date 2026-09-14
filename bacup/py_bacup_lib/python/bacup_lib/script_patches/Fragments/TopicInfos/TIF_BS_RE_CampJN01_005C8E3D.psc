Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && HighPoweredMagnet != None
        Game.GetPlayer().RemoveItem(HighPoweredMagnet, 1, True, akSpeakerRef)
    EndIf
EndFunction
