Event OnOpen(ObjectReference akActionRef)
    Actor player = OwningPlayer.GetActorReference()
    If player == None
        Return
    EndIf
    If player.GetValue(W05_MQ_003P_Muscle_DnD_FrontDoorUnlockedOnce) >= 1.0
        Return
    EndIf
    player.SetValue(W05_MQ_003P_Muscle_DnD_FrontDoorUnlockedOnce, 1.0)
EndEvent
