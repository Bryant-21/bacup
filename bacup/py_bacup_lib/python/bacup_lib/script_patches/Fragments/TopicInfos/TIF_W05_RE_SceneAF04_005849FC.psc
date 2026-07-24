Function Fragment_End(ObjectReference akSpeakerRef)
    If Ammo44 != None
        Game.GetPlayer().RemoveItem(Ammo44, 10, true, akSpeakerRef)
    EndIf
EndFunction
