Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef != None && W05_BethyMangano_GaveSuppliesAV != None && akSpeakerRef.GetValue(W05_BethyMangano_GaveSuppliesAV) == 0
        If HighIntRock != None
            Game.GetPlayer().AddItem(HighIntRock, 1)
        EndIf
        akSpeakerRef.SetValue(W05_BethyMangano_GaveSuppliesAV, 1.0)
    EndIf
EndFunction
