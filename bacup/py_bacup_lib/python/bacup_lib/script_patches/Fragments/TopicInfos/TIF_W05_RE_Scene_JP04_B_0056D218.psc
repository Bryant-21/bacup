Function Fragment_End(ObjectReference akSpeakerRef)
    If CapsRef != None
        Game.GetPlayer().RemoveItem(CapsRef, 25, true, akSpeakerRef)
    EndIf
EndFunction
