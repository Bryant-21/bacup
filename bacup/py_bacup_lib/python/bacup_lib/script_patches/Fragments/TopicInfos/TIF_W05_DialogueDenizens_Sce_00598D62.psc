Function Fragment_End(ObjectReference akSpeakerRef)
    If LL_TreasureMap != None
        Game.GetPlayer().AddItem(LL_TreasureMap, 1)
    EndIf
EndFunction
