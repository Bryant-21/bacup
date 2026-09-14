Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && Typewriter != None
        Game.GetPlayer().RemoveItem(Typewriter, 1, True, akSpeakerRef)
    EndIf
EndFunction
