Function Fragment_End(ObjectReference akSpeakerRef)
    If PlayerHostileFaction != None
        (akSpeakerRef as Actor).AddToFaction(PlayerHostileFaction)
    EndIf
EndFunction
