Event OnDeath(Actor akKiller)
    Actor playerRef = OwningPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Actor solRef = Sol.GetActorReference()
    If playerRef != None && solRef != None && akKiller == solRef && playerRef.GetValue(W05_MQ_004P_Crane_SolKilledCrane) < 1.0
        playerRef.SetValue(W05_MQ_004P_Crane_SolKilledCrane, 1.0)
    EndIf
EndEvent
