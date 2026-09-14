Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && RepChangeGlobal != None
        Game.GetPlayer().ModValue(ReputationAV, RepChangeGlobal.GetValue())
    EndIf
EndFunction
