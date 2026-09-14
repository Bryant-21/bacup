Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && GiddyupButtercupHead != None
        Game.GetPlayer().RemoveItem(GiddyupButtercupHead, 1, True, akSpeakerRef)
    EndIf
EndFunction
