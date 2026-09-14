Function Fragment_Begin(ObjectReference akSpeakerRef)
    If akSpeakerRef != None
        akSpeakerRef.SetValue(COMP_Chef_AV_LastDined, Utility.GetCurrentGameTime())
    EndIf
EndFunction
