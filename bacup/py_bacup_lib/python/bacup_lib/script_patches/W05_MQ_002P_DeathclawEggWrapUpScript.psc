Event OnEnd(ObjectReference akSpeakerRef, bool abHasBeenSaid)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    If RequireEgg
        If playerRef.GetItemCount(DeathclawEgg) > 0
            playerRef.RemoveItem(DeathclawEgg, 1, True)
            playerRef.AddItem(StealthBoy, 1, False)
        EndIf
    Else
        playerRef.AddItem(DeathclawEgg, 1, False)
    EndIf
    playerRef.SetValue(W05_MQ_002P_Radical_TylerCounty_LiebowtizSceneIndex, 4.0)
EndEvent
