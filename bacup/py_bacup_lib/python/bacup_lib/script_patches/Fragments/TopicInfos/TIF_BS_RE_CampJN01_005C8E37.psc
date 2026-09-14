Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && c_Circuitry_scrap != None
        Game.GetPlayer().RemoveItem(c_Circuitry_scrap, 1, True, akSpeakerRef)
    EndIf
EndFunction
