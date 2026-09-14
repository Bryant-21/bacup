Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && Microscope != None
        Game.GetPlayer().RemoveItem(Microscope, 1, True, akSpeakerRef)
    EndIf
EndFunction
