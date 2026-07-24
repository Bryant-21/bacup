Event OnOpen(ObjectReference akActionRef)
    If hasSwarmSpawned || BeeSwarm == None
        Return
    EndIf

    If Utility.RandomFloat(0.0, 1.0) > SpawnChance
        Return
    EndIf

    Actor spawnedSwarm = PlaceActorAtMe(BeeSwarm)
    If spawnedSwarm != None
        hasSwarmSpawned = True
    EndIf
EndEvent
