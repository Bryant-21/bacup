Function Fragment_Begin(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(KnowsAboutSideBusinessAV, 1.0)
    EndIf
EndFunction
