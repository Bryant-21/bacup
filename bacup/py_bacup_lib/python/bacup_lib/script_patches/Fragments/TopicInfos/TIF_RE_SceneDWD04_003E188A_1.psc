Function Fragment_End(ObjectReference akSpeakerRef)
    If MoldyFood
        Game.GetPlayer().AddItem(MoldyFood, 1)
    EndIf
    If GeneralsSteakHouseMarker
        GeneralsSteakHouseMarker.AddToMap()
    EndIf
EndFunction
