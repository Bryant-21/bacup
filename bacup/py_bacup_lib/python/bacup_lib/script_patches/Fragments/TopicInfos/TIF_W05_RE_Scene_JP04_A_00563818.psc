Function Fragment_Begin(ObjectReference akSpeakerRef)
    If RewardRef != None
        Game.GetPlayer().AddItem(RewardRef, 1)
    EndIf
EndFunction
