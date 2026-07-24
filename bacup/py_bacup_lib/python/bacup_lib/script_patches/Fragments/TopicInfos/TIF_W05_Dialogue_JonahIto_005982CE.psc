Function Fragment_End(ObjectReference akSpeakerRef)
    if akSpeakerRef == None
        return
    endif
    akSpeakerRef.SetValue(W05_JonahIto_MetPlayerAV, 1.0)
EndFunction
