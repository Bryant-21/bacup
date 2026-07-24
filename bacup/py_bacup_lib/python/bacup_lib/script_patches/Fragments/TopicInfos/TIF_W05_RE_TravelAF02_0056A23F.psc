Function Fragment_End(ObjectReference akSpeakerRef)
    If RadAway != None
        Game.GetPlayer().RemoveItem(RadAway, 1, true, akSpeakerRef)
    EndIf
EndFunction
