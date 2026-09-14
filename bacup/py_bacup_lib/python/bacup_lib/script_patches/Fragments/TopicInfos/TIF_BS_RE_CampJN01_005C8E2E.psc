Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && Toaster_01_PostWar != None
        Game.GetPlayer().RemoveItem(Toaster_01_PostWar, 1, True, akSpeakerRef)
    EndIf
EndFunction
