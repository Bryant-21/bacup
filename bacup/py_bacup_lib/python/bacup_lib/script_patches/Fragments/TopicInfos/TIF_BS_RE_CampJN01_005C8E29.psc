Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && CircuitBoardMilitary != None
        Game.GetPlayer().RemoveItem(CircuitBoardMilitary, 1, True, akSpeakerRef)
    EndIf
EndFunction
