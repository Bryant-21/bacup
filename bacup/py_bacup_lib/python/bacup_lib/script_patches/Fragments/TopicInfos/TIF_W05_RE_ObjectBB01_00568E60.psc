Function Fragment_Begin(ObjectReference akSpeakerRef)
    If ToyAlien != None
        Game.GetPlayer().RemoveItem(ToyAlien, 1, true, akSpeakerRef)
    EndIf
    If ToyForPlayer != None
        akSpeakerRef.RemoveItem(ToyForPlayer, 1, true, Game.GetPlayer())
    EndIf
    If PlayerAlias != None
        PlayerAlias.ForceRefTo(Game.GetPlayer())
    EndIf
EndFunction
