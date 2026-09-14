Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(AV_StrengthEnduranceAgility, 1.0)
    EndIf
EndFunction
