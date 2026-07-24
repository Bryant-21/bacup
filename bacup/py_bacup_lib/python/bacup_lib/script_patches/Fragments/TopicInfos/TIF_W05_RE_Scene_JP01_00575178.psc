Function Fragment_Begin(ObjectReference akSpeakerRef)
    If FoodRef != None
        Game.GetPlayer().AddItem(FoodRef, 1)
    EndIf
    If WaterRef != None
        Game.GetPlayer().AddItem(WaterRef, 1)
    EndIf
EndFunction
