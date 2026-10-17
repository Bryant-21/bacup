; RE_TravelTS02 Sheepsquatch trail: myEffect is laid every EffectInterval from StartPoint toward SpawnPoint, jittered
; up to Offset sideways, and collected in Effects; shutdown removes the trail.
Event OnQuestInit()
    StartPointRef = StartPoint.GetReference()
    SpawnPointRef = SpawnPoint.GetReference()
    If StartPointRef == None || SpawnPointRef == None || myEffect == None || EffectInterval <= 0.0
        Return
    EndIf
    Float dx = SpawnPointRef.GetPositionX() - StartPointRef.GetPositionX()
    Float dy = SpawnPointRef.GetPositionY() - StartPointRef.GetPositionY()
    Float dz = SpawnPointRef.GetPositionZ() - StartPointRef.GetPositionZ()
    Float pathLength = Math.Sqrt(dx * dx + dy * dy)
    If pathLength <= EffectInterval
        Return
    EndIf
    Float travelled = EffectInterval
    While travelled < pathLength
        Float t = travelled / pathLength
        ObjectReference effectRef = StartPointRef.PlaceAtMe(myEffect)
        If effectRef != None
            Float x = StartPointRef.GetPositionX() + dx * t + Utility.RandomFloat(-Offset, Offset)
            Float y = StartPointRef.GetPositionY() + dy * t + Utility.RandomFloat(-Offset, Offset)
            effectRef.SetPosition(x, y, StartPointRef.GetPositionZ() + dz * t)
            effectRef.MoveToNearestNavmeshLocation()
            Effects.AddRef(effectRef)
        EndIf
        travelled += EffectInterval
    EndWhile
EndEvent

Event OnQuestShutdown()
    Int index = Effects.GetCount() - 1
    While index >= 0
        ObjectReference effectRef = Effects.GetAt(index)
        If effectRef != None
            effectRef.Disable()
            effectRef.Delete()
        EndIf
        index -= 1
    EndWhile
    Effects.RemoveAll()
EndEvent
