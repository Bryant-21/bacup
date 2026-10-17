; CBZ04 reclaim gear: the objective points at the real container and its decoys. Reaching a decoy within
; DiscoveryDistance drops it from the objective (redisplaying RedisplayObjectiveOnDecoyDiscovery); reaching the real
; container sets StageToSetOnDiscoverRealObject.
Event OnAliasInit()
    StartTimer(2.0)
EndEvent

Event OnTimer(Int aiTimerID)
    Quest owner = GetOwningQuest()
    If !owner.IsRunning() || owner.IsStageDone(StageToSetOnDiscoverRealObject)
        Return
    EndIf
    Actor player = Game.GetPlayer()
    ObjectReference realRef = RealObject.GetReference()
    If realRef != None && player.GetDistance(realRef) <= DiscoveryDistance
        owner.SetStage(StageToSetOnDiscoverRealObject)
        Return
    EndIf
    Int index = GetCount() - 1
    While index >= 0
        ObjectReference decoy = GetAt(index)
        If decoy != None && decoy != realRef && player.GetDistance(decoy) <= DiscoveryDistance
            RemoveRef(decoy)
            If RedisplayObjectiveOnDecoyDiscovery >= 0
                owner.SetObjectiveDisplayed(RedisplayObjectiveOnDecoyDiscovery, True, True)
            EndIf
        EndIf
        index -= 1
    EndWhile
    StartTimer(2.0)
EndEvent
