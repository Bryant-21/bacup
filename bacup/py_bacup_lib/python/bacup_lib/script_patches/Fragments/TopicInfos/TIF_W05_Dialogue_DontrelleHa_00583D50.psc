Function Fragment_End(ObjectReference akSpeakerRef)
    If W05_PlayerHasTalkedToDontrelleHaines != None
        Game.GetPlayer().SetValue(W05_PlayerHasTalkedToDontrelleHaines, 1.0)
    EndIf
EndFunction
