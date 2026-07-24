Function Fragment_Begin(ObjectReference akSpeakerRef)
    If CapsRef != None
        Game.GetPlayer().RemoveItem(CapsRef, 15, true, akSpeakerRef)
    EndIf
EndFunction
