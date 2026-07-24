Function Fragment_Begin(ObjectReference akSpeakerRef)
    If AmmoFusionCore != None
        Game.GetPlayer().RemoveItem(AmmoFusionCore, 1, true, akSpeakerRef)
    EndIf
EndFunction
