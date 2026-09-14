Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(GumleyAV, 1.0)
    EndIf
EndFunction
