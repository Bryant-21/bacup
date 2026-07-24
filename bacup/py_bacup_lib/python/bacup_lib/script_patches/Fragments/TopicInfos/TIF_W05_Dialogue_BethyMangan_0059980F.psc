Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef != None && W05_BethyMangano_GaveSuppliesAV != None && akSpeakerRef.GetValue(W05_BethyMangano_GaveSuppliesAV) == 0
        If ScienceMagazine != None
            Game.GetPlayer().AddItem(ScienceMagazine, 1)
        EndIf
        akSpeakerRef.SetValue(W05_BethyMangano_GaveSuppliesAV, 1.0)
    EndIf
EndFunction
