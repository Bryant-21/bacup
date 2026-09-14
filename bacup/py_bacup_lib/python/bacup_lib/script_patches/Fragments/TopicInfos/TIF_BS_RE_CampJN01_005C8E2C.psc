Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && SensorModule != None
        Game.GetPlayer().RemoveItem(SensorModule, 1, True, akSpeakerRef)
    EndIf
EndFunction
