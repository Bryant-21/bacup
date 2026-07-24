Function Fragment_End(ObjectReference akSpeakerRef)
    If Stimpak != None
        Game.GetPlayer().RemoveItem(Stimpak, 1, true, akSpeakerRef)
    EndIf
EndFunction
