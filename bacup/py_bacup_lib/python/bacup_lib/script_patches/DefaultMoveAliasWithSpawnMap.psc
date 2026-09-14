Event OnQuestInit()
    ObjectReference kCenter = SpawnMapCenterMarker.GetReference()
    ObjectReference kActor = ActorToMove.GetReference()
    If kCenter == None || kActor == None
        Return
    EndIf

    Float minDistance = SpawnMapMinDistance
    Float maxDistance = SpawnMapMaxDistance
    If minDistance < 0.0
        minDistance = 0.0
    EndIf
    If maxDistance < 0.0
        maxDistance = 0.0
    EndIf
    If maxDistance < minDistance
        Float oldMinDistance = minDistance
        minDistance = maxDistance
        maxDistance = oldMinDistance
    EndIf

    If maxDistance == 0.0
        kActor.MoveTo(kCenter, 0.0, 0.0, 0.0, False)
    Else
        Float spawnDistance = Utility.RandomFloat(minDistance, maxDistance)
        Float spawnAngle = Utility.RandomFloat(0.0, 360.0)
        kActor.MoveTo(kCenter, Math.Cos(spawnAngle) * spawnDistance, Math.Sin(spawnAngle) * spawnDistance, 0.0, False)
    EndIf

    If StageToSetOnSpawn >= 0
        SetStage(StageToSetOnSpawn)
    EndIf
EndEvent
