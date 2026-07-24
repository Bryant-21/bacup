Function Fragment_End(ObjectReference akSpeakerRef)
    If RemoveRef != None
        Game.GetPlayer().RemoveItem(RemoveRef, 20)
    EndIf
    If RewardRef1 != None
        Game.GetPlayer().AddItem(RewardRef1, 1)
    EndIf
    If RewardRef2 != None
        Game.GetPlayer().AddItem(RewardRef2, 1)
    EndIf
EndFunction
