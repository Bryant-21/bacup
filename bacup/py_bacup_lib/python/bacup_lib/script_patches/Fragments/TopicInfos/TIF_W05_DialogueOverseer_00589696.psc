Function Fragment_End(ObjectReference akSpeakerRef)
    If W05_OverseerEpilogueSceneDone != None
        Game.GetPlayer().SetValue(W05_OverseerEpilogueSceneDone, 1.0)
    EndIf
EndFunction
