Function Fragment_Begin(ObjectReference akSpeakerRef)
    If LuckGiveObject != None && Game.GetPlayer() != None
        Game.GetPlayer().AddItem(LuckGiveObject, 1, False)
    EndIf
EndFunction
