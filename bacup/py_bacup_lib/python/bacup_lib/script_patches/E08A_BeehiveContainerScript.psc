Event OnActivate(ObjectReference akActionRef)
    If BeeSwarm == None || CurrentCell == None
        Return
    EndIf
    If enabled
        Return
    EndIf
    enabled = True
    If MaxActorCount <= 0 || MaxSpawnCount <= 0
        Return
    EndIf
    If spawnedSwarms == None
        spawnedSwarms = new Actor[0]
    EndIf
    If spawnedSwarms.Length >= MaxActorCount
        Return
    EndIf
    If SpawnTime > 0.0
        Utility.Wait(SpawnTime)
    EndIf
    Int spawnedThisTrigger = 0
    While spawnedThisTrigger < MaxSpawnCount && spawnedSwarms.Length < MaxActorCount
        Actor newSwarm = Self.PlaceActorAtMe(BeeSwarm) as Actor
        If newSwarm != None
            If AppliedFaction != None
                newSwarm.AddToFaction(AppliedFaction)
            EndIf
            If PreferredTargetCollection != None
                PreferredTargetCollection.AddRef(newSwarm)
            EndIf
            spawnedSwarms.Add(newSwarm)
            If SpawnOnDeath
                RegisterForRemoteEvent(newSwarm, "OnDeath")
            EndIf
        EndIf
        spawnedThisTrigger += 1
    EndWhile
EndEvent
