Function Fragment_Begin(ObjectReference akSpeakerRef)
    If Alias_ScenePlayer != None && Game.GetPlayer() != None
        Alias_ScenePlayer.ForceRefTo(Game.GetPlayer())
    EndIf
EndFunction
