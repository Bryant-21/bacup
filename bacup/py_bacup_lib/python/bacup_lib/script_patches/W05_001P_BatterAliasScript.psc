Event OnDeath(Actor akKiller)
    Actor playerRef = OwningPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef == None
        Return
    EndIf

    Actor mortRef = Mort.GetActorReference()
    If akKiller == playerRef
        If playerRef.GetValue(W05_MQ_001P_Wayward_PlayerKilledBatter) < 1.0
            playerRef.SetValue(W05_MQ_001P_Wayward_PlayerKilledBatter, 1.0)
        EndIf
    ElseIf mortRef != None && akKiller == mortRef
        If playerRef.GetValue(W05_MQ_001P_Wayward_MortKilledBatter) < 1.0
            playerRef.SetValue(W05_MQ_001P_Wayward_MortKilledBatter, 1.0)
        EndIf
    EndIf
EndEvent
