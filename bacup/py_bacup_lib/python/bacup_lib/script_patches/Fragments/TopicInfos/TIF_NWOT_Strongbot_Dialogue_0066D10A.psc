Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(pNWOT_Strongbot_AboutTestStrength_AV, 1.0)
    EndIf
EndFunction
