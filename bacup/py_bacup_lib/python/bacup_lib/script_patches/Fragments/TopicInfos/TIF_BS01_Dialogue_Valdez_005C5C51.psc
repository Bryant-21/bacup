Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(BoSz01_PlayerSpokeToValdez, 1.0)
        Game.GetPlayer().SetValue(BoSz01_PlayerKACacheDepot, 1.0)
    EndIf
EndFunction
