Function Fragment_End(ObjectReference akSpeakerRef)
    If CapsRef != None
        Game.GetPlayer().RemoveItem(CapsRef, 20)
    EndIf
    If RewardRef != None
        Game.GetPlayer().AddItem(RewardRef, 1)
    EndIf
EndFunction
