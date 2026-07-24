Function Fragment_Begin(ObjectReference akSpeakerRef)
    If ScrapRef1 != None
        Game.GetPlayer().AddItem(ScrapRef1, 1)
    EndIf
    If WoodRef != None
        Game.GetPlayer().AddItem(WoodRef, 1)
    EndIf
    If ScrapRef2 != None
        Game.GetPlayer().AddItem(ScrapRef2, 1)
    EndIf
EndFunction
