Function Fragment_End(ObjectReference akSpeakerRef)
    If PlayersDoneRef != None && Game.GetPlayer() != None
        PlayersDoneRef.AddRef(Game.GetPlayer())
    EndIf
EndFunction
