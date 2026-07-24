Function Fragment_End(ObjectReference akSpeakerRef)
    If CapsRef != None
        Game.GetPlayer().RemoveItem(CapsRef, 10, true, akSpeakerRef)
    EndIf
EndFunction
