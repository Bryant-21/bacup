Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && AlarmClock != None
        Game.GetPlayer().RemoveItem(AlarmClock, 1, True, akSpeakerRef)
    EndIf
EndFunction
