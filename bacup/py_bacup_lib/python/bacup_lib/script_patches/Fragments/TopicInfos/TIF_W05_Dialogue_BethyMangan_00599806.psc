Function Fragment_End(ObjectReference akSpeakerRef)
    if akSpeakerRef == None
        return
    endif
    akSpeakerRef.SetValue(W05_BethyMangano_IsSmartAV, 1.0)
EndFunction
