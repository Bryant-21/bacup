Function Fragment_End(ObjectReference akSpeakerRef)
    If c_FiberOptics_scrap != None
        Game.GetPlayer().RemoveItem(c_FiberOptics_scrap, 1, true, akSpeakerRef)
    EndIf
EndFunction
