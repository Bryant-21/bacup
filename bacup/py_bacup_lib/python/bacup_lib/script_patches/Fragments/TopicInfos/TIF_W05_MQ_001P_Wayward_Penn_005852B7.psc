Function Fragment_End(ObjectReference akSpeakerRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef.GetValue(MQ_OverseerHolotape01PickedUp) < 1.0
        If playerRef.GetItemCount(MQ_Overseer_01_Vault76Holotape) == 0
            playerRef.AddItem(MQ_Overseer_01_Vault76Holotape, 1, False)
        EndIf
        playerRef.SetValue(MQ_OverseerHolotape01PickedUp, 1.0)
    EndIf
EndFunction
