Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(pNWOT_Pat_AfterBoss_AV, 1.0)
    EndIf
EndFunction
