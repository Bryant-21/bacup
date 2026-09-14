Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && Radio_Jammer != None
        Game.GetPlayer().RemoveItem(Radio_Jammer, 1, True, akSpeakerRef)
    EndIf
EndFunction
