Function Fragment_End(ObjectReference akSpeakerRef)
    If Ammo38Caliber != None
        Game.GetPlayer().RemoveItem(Ammo38Caliber, 10, true, akSpeakerRef)
    EndIf
EndFunction
