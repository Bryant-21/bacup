Function Fragment_End(ObjectReference akSpeakerRef)
    If RE_TravelCT01_Note != None && Game.GetPlayer() != None
        Game.GetPlayer().AddItem(RE_TravelCT01_Note, 1, False)
    EndIf
EndFunction
