Function Fragment_End(ObjectReference akSpeakerRef)
    If MTR04_MrFuzzyToken != None
        Game.GetPlayer().AddItem(MTR04_MrFuzzyToken, 1)
    EndIf
EndFunction
