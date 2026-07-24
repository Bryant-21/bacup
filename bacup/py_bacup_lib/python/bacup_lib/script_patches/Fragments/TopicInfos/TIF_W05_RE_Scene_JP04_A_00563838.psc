Function Fragment_Begin(ObjectReference akSpeakerRef)
    If CapsRef != None
        Game.GetPlayer().RemoveItem(CapsRef, 5, true, akSpeakerRef)
    EndIf
EndFunction
