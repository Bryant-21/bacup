Function Fragment_End(ObjectReference akSpeakerRef)
    If WaterDirty != None
        Game.GetPlayer().RemoveItem(WaterDirty, 1, true, akSpeakerRef)
    EndIf
EndFunction
