Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(AV_PlayerKnows_Emerson, 1.0)
        Game.GetPlayer().SetValue(AV_PlayerUsed_Charisma, 1.0)
        Game.GetPlayer().SetValue(AV_Luck, 1.0)
    EndIf
EndFunction
