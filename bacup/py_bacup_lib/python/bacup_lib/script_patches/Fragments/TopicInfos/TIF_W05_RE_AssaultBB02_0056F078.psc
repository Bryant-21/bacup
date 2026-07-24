Function Fragment_Begin(ObjectReference akSpeakerRef)
    If CurrentSpeakerScene != None
        CurrentSpeakerScene.ForceRefTo(akSpeakerRef)
    EndIf
EndFunction
