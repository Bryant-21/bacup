Function Fragment_End(ObjectReference akSpeakerRef)
    If CapsRef != None
        Game.GetPlayer().RemoveItem(CapsRef, 30, true, akSpeakerRef)
    EndIf
EndFunction
