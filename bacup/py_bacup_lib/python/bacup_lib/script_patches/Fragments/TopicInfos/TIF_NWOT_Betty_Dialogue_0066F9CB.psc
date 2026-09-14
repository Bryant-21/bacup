Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(HasSpokenToAV, 1.0)
    EndIf
EndFunction
