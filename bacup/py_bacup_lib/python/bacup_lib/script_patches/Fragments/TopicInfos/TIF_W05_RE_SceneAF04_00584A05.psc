Function Fragment_End(ObjectReference akSpeakerRef)
    If Ammo10mm != None
        Game.GetPlayer().RemoveItem(Ammo10mm, 10, true, akSpeakerRef)
    EndIf
EndFunction
