Function Fragment_End(ObjectReference akSpeakerRef)
    if akSpeakerRef == None
        return
    endif
    akSpeakerRef.SetValue(W05_BethyMangano_KnownAV, 1.0)
EndFunction
