Function Fragment_Begin(ObjectReference akSpeakerRef)
    If CurRenterAlias != None && Game.GetPlayer() != None
        CurRenterAlias.ForceRefTo(Game.GetPlayer())
    EndIf
EndFunction
