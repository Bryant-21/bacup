Function Fragment_Begin(ObjectReference akSpeakerRef)
    If akSpeakerRef != None && AV_Cooldown != None
        akSpeakerRef.SetValue(AV_Cooldown, Utility.GetCurrentGameTime())
    EndIf
EndFunction
