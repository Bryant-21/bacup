Function Fragment_End(ObjectReference akSpeakerRef)
    If Ammo556 != None
        Game.GetPlayer().RemoveItem(Ammo556, 10, true, akSpeakerRef)
    EndIf
EndFunction
