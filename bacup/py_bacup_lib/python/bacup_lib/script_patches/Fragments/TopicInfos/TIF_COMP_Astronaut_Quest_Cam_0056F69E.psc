Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(AV_Give_Automatron, 1.0)
    EndIf
EndFunction
