Event OnOpen(ObjectReference akActionRef)
    If OwningPlayer
        ObjectReference playerRef = OwningPlayer.GetRef()
        If playerRef && playerRef.GetValue(W05_MQ_003P_Muscle_SkinnerAcknowledgesBreakIn) == 0.0
            Bool hasCredential = playerRef.GetItemCount(KeyObject) > 0 || playerRef.GetItemCount(AccessCard) > 0
            If AccessCard01
                hasCredential = hasCredential || playerRef.GetItemCount(AccessCard01) > 0
            EndIf
            If !hasCredential
                playerRef.SetValue(W05_MQ_003P_Muscle_SkinnerAcknowledgesBreakIn, 1.0)
            EndIf
        EndIf
    EndIf
EndEvent
