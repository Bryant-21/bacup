Event OnDeath(Actor akKiller)
    Actor player = None
    If OwningPlayer != None
        player = OwningPlayer.GetActorReference()
    EndIf
    If player == None || akKiller != player
        Return
    EndIf
    If W05_MQ_002P_Radical_PlayerKilledRoper != None
        player.SetValue(W05_MQ_002P_Radical_PlayerKilledRoper, 1.0)
    EndIf
EndEvent
