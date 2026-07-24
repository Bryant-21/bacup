Event OnTriggerEnter(ObjectReference akActionRef)
    If GetTriggerObjectCount() != 1 || akActionRef != Game.GetPlayer()
        Return
    EndIf
    If CAMPCheck != None && GetValue(CAMPCheck) != 0.0
        Return
    EndIf

    Float chance = COMP_AllySpawnChance_Standard.GetValue()
    If SpawnChanceOverride != None
        chance = SpawnChanceOverride.GetValue()
    EndIf

    FormList spawnList = COMP_AllySpawnMarker_List_Default
    If List_Override != None
        spawnList = List_Override
    EndIf

    If Utility.RandomFloat(0.0, 100.0) < chance && spawnList != None && spawnList.GetSize() > 0
        ActorBase allyBase = spawnList.GetAt(Utility.RandomInt(0, spawnList.GetSize() - 1)) as ActorBase
        If allyBase != None
            PlaceActorAtMe(allyBase)
        EndIf
    EndIf

    Disable()
EndEvent
