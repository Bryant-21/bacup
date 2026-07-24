Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef != None && W05_JonahIto_GaveSuppliesAV != None && akSpeakerRef.GetValue(W05_JonahIto_GaveSuppliesAV) == 0
        If W05_JonahIto_LL_GiveToPlayer != None
            Game.GetPlayer().AddItem(W05_JonahIto_LL_GiveToPlayer, 1)
        EndIf
        akSpeakerRef.SetValue(W05_JonahIto_GaveSuppliesAV, 1.0)
    EndIf
    If akSpeakerRef != None && W05_JonahIto_MetPlayerAV != None
        akSpeakerRef.SetValue(W05_JonahIto_MetPlayerAV, 1.0)
    EndIf
EndFunction
