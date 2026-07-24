Function Fragment_End(ObjectReference akSpeakerRef)
    If c_Circuitry_scrap != None
        Game.GetPlayer().RemoveItem(c_Circuitry_scrap, 1, true, akSpeakerRef)
    EndIf
EndFunction
