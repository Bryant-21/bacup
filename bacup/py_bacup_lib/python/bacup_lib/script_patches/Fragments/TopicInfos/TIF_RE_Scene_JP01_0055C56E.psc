Function Fragment_End(ObjectReference akSpeakerRef)
    If PlayerAlias != None && Game.GetPlayer() != None
        PlayerAlias.ForceRefTo(Game.GetPlayer())
    EndIf
EndFunction
