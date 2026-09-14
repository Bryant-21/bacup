Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && Food_Dehydrator != None
        Game.GetPlayer().RemoveItem(Food_Dehydrator, 1, True, akSpeakerRef)
    EndIf
EndFunction
