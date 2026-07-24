Function Fragment_End(ObjectReference akSpeakerRef)
    if akSpeakerRef == None
        return
    endif
    akSpeakerRef.SetValue(W05_SunnyMisc_Insult, 1.0)
EndFunction
