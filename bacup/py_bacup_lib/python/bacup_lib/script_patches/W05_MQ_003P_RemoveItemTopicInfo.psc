Event OnEnd(ObjectReference akSpeakerRef, bool abHasBeenSaid)
    If OwningPlayer
        ObjectReference playerRef = OwningPlayer.GetRef()
        If playerRef && playerRef.GetItemCount(ItemToRemove) > 0
            playerRef.RemoveItem(ItemToRemove, 1, True)
            playerRef.SetValue(W05_MQ_003P_Muscle_PlayerGaveSolItem, 1.0)
        EndIf
    EndIf
EndEvent
