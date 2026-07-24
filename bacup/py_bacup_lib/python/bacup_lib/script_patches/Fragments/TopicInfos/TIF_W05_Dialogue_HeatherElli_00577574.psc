Function Fragment_End(ObjectReference akSpeakerRef)
    If W05_PlayerHasTalkedToHeatherEllis != None
        Game.GetPlayer().SetValue(W05_PlayerHasTalkedToHeatherEllis, 1.0)
    EndIf
EndFunction
