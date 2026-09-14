Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().ModValue(AV_FlirtCount, 1.0)
    EndIf
EndFunction
