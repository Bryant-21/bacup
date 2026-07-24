Function Fragment_End(ObjectReference akSpeakerRef)
    If RewardRef != None
        Game.GetPlayer().AddItem(RewardRef, 1)
    EndIf
EndFunction
