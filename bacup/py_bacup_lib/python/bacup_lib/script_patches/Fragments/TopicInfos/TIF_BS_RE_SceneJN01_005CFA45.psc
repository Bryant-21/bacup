Function Fragment_Begin(ObjectReference akSpeakerRef)
    If akSpeakerRef != None
        akSpeakerRef.ModValue(BS_RE_TravelJN01_AV_Counter, 1.0)
    EndIf
EndFunction
