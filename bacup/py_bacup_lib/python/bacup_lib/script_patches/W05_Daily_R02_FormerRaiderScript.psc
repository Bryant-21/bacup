Event OnDeath(Actor akKiller)
    Quest ownerQuest = Self.GetOwningQuest()
    If ownerQuest == None
        Return
    EndIf

    Actor player = Alias_Player.GetActorReference()
    If akKiller != None && player != None && akKiller == player
        ownerQuest.SetStage(PlayerKilledStage)
        Return
    EndIf

    Actor myActor = Self.GetActorReference()
    If player != None && myActor != None
        If player.GetDistance(myActor) <= (DistanceCheck as Float)
            ownerQuest.SetStage(OtherKilledStage)
        EndIf
    EndIf
EndEvent
