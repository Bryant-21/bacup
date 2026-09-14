Function Fragment_End(ObjectReference akSpeakerRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef
        playerRef.AddItem(StealthBoy, 1, False)
        playerRef.SetValue(W05_MQ_002P_Radical_TylerCounty_LiebowtizSceneIndex, 4.0)
    EndIf
EndFunction
