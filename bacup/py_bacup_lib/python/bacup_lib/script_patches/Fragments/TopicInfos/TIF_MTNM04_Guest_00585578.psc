Function Fragment_Begin(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(pMTNM04_InterviewedAV, 1.0)
    EndIf
EndFunction
