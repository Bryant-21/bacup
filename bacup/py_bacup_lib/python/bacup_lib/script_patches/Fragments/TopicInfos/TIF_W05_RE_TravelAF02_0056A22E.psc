Function Fragment_End(ObjectReference akSpeakerRef)
    If RadX != None
        Game.GetPlayer().RemoveItem(RadX, 1, true, akSpeakerRef)
    EndIf
EndFunction
