Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(AV_MQ02_PE, 1.0)
    EndIf
EndFunction
