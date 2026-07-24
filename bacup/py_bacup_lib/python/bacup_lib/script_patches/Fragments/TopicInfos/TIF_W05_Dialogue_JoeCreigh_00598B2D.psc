Function Fragment_End(ObjectReference akSpeakerRef)
    If Ammo308Caliber != None
        Game.GetPlayer().AddItem(Ammo308Caliber, 1)
    EndIf
EndFunction
