Function Fragment_Begin(ObjectReference akSpeakerRef)
    If MirelurkSoftshellMeatRef != None && Game.GetPlayer() != None
        Game.GetPlayer().RemoveItem(MirelurkSoftshellMeatRef, 1, True, akSpeakerRef)
    EndIf
EndFunction
