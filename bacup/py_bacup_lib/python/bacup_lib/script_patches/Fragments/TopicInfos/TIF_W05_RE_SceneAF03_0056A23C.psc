Function Fragment_Begin(ObjectReference akSpeakerRef)
    Actor speaker = akSpeakerRef as Actor
    If REPlayerEnemy != None && speaker != None
        speaker.AddToFaction(REPlayerEnemy)
    EndIf
EndFunction
