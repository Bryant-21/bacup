Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef != None
        akSpeakerRef.SetValue(XPD_Pitt02_DaniloGaveRadGear, 1.0)
        Game.GetPlayer().AddItem(Headwear_Gasmask, 1, False)
    EndIf
EndFunction
