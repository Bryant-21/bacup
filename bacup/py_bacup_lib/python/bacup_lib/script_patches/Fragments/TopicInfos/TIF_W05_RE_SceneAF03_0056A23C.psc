Function Fragment_Begin(ObjectReference akSpeakerRef)
    If REPlayerEnemy != None
        akSpeakerRef.AddToFaction(REPlayerEnemy)
    EndIf
EndFunction
