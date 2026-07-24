Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef != None && W05_Wayward_RC_MortMostRecentClue != None
        akSpeakerRef.SetValue(W05_Wayward_RC_MortMostRecentClue, NewValue)
    EndIf
EndFunction
