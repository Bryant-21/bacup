Function Fragment_End(ObjectReference akSpeakerRef)
    If RemoveDrug != None
        Game.GetPlayer().RemoveItem(RemoveDrug, 1)
    EndIf
    If W05_Denizen_LL_Chems != None
        Game.GetPlayer().AddItem(W05_Denizen_LL_Chems, 1)
    EndIf
EndFunction
