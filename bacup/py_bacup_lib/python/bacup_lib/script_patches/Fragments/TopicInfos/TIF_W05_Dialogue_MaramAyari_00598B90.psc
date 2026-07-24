Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef != None && W05_MaramAyari_GaveSuppliesAV != None && akSpeakerRef.GetValue(W05_MaramAyari_GaveSuppliesAV) == 0
        If W05_MaramAyari_LL_GiveToPlayer != None
            Game.GetPlayer().AddItem(W05_MaramAyari_LL_GiveToPlayer, 1)
        EndIf
        akSpeakerRef.SetValue(W05_MaramAyari_GaveSuppliesAV, 1.0)
    EndIf
EndFunction
