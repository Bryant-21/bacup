Function Fragment_Begin(ObjectReference akSpeakerRef)
    If RewardRef1 != None
        Game.GetPlayer().AddItem(RewardRef1, 1)
    EndIf
    If RewardRef2 != None
        Game.GetPlayer().AddItem(RewardRef2, 1)
    EndIf
EndFunction
