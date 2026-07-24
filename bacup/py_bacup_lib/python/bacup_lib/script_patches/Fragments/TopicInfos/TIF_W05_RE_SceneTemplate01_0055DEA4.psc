Function Fragment_End(ObjectReference akSpeakerRef)
    If WaterBoiled != None && Game.GetPlayer().GetItemCount(WaterBoiled) > 0
        Game.GetPlayer().RemoveItem(WaterBoiled, 1, true, akSpeakerRef)
    ElseIf WaterPurified != None && Game.GetPlayer().GetItemCount(WaterPurified) > 0
        Game.GetPlayer().RemoveItem(WaterPurified, 1, true, akSpeakerRef)
    EndIf
EndFunction
