Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef != None && W05_GilbertHopsonGaveSuppliesAV != None && akSpeakerRef.GetValue(W05_GilbertHopsonGaveSuppliesAV) == 0
        If BerryMentats != None
            Game.GetPlayer().AddItem(BerryMentats, 1)
        EndIf
        akSpeakerRef.SetValue(W05_GilbertHopsonGaveSuppliesAV, 1.0)
    EndIf
    If akSpeakerRef != None && W05_GilbertHopsonMetPlayerAV != None
        akSpeakerRef.SetValue(W05_GilbertHopsonMetPlayerAV, 1.0)
    EndIf
EndFunction
