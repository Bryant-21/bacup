Function Fragment_End(ObjectReference akSpeakerRef)
    If Ammo308Caliber != None
        Game.GetPlayer().RemoveItem(Ammo308Caliber, 10, true, akSpeakerRef)
    EndIf
EndFunction
