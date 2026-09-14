Function Fragment_Begin(ObjectReference akSpeakerRef)
    If PlayerFinishedRE != None && Game.GetPlayer() != None
        PlayerFinishedRE.AddRef(Game.GetPlayer())
    EndIf
EndFunction
