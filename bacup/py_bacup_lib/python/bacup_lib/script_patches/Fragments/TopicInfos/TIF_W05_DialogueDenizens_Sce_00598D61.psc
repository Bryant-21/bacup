Function Fragment_End(ObjectReference akSpeakerRef)
    If RemoveRef != None
        Game.GetPlayer().RemoveItem(RemoveRef, 1)
    EndIf
    If RewardRef1 != None
        Game.GetPlayer().AddItem(RewardRef1, 1)
    EndIf
EndFunction
