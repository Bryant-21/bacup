Function Fragment_End(ObjectReference akSpeakerRef)
    Actor akSpeaker = akSpeakerRef as Actor
    If akSpeaker
        akSpeaker.ShowBarterMenu()
    EndIf
EndFunction
