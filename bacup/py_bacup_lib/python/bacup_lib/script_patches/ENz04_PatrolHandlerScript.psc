Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    ReconcilePatrols()
EndEvent

Bool Function TransferPatrol(ObjectReference akCenter, RefCollectionAlias akPatrol)
    If akCenter == None || akPatrol == None || akPatrol.GetCount() == 0
        Return False
    EndIf
    If !IsRunning() && !Start()
        Return False
    EndIf
    Int index = 0
    While PatrolData != None && index < PatrolData.Length
        PatrolDatum patrol = PatrolData[index]
        If patrol.PatrolCenterMarker == akCenter && patrol.PatrolCollection != None
            Enz04_PatrolCollectionScript collection = patrol.PatrolCollection as Enz04_PatrolCollectionScript
            If collection == None
                Return False
            EndIf
            collection.bCleanUpCollection = False
            Int actorIndex = akPatrol.GetCount() - 1
            While actorIndex >= 0
                Actor patrolBot = akPatrol.GetAt(actorIndex) as Actor
                If patrolBot != None && patrolBot != Game.GetPlayer()
                    patrol.PatrolCollection.AddRef(patrolBot)
                    akPatrol.RemoveRef(patrolBot)
                    If !patrolBot.IsDead()
                        patrolBot.SetUnconscious(False)
                    EndIf
                EndIf
                actorIndex -= 1
            EndWhile
            If patrol.PatrolCollection.GetCount() == 0
                Return False
            EndIf
            If PatrolCenterMarkers != None
                PatrolCenterMarkers.AddRef(akCenter)
            EndIf
            patrol.PatrolCollection.EvaluateAll()
            StartTimer(iPatrolTimerLength as Float, index)
            StartTimer(iPatrolFailsafeTimerLength as Float, index + iFailsafeIDOffset)
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Event OnTimer(Int aiTimerID)
    Int index = aiTimerID
    Bool forceRetirement = False
    If index >= iFailsafeIDOffset
        index -= iFailsafeIDOffset
        forceRetirement = True
    EndIf
    If PatrolData == None || index < 0 || index >= PatrolData.Length
        Return
    EndIf
    Enz04_PatrolCollectionScript collection = PatrolData[index].PatrolCollection as Enz04_PatrolCollectionScript
    If collection == None
        Return
    EndIf
    collection.bCleanUpCollection = True
    If forceRetirement && ENz04_FailsafeKill != None
        Int actorIndex = 0
        Bool announced = False
        While actorIndex < collection.GetCount() && !announced
            Actor patrolBot = collection.GetAt(actorIndex) as Actor
            If patrolBot != None && !patrolBot.IsDead()
                patrolBot.Say(ENz04_FailsafeKill)
                announced = True
            EndIf
            actorIndex += 1
        EndWhile
    EndIf
    collection.CleanupPatrol(forceRetirement)
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        ReconcilePatrols()
    EndIf
EndEvent

Function ReconcilePatrols()
    Int index = 0
    While PatrolData != None && index < PatrolData.Length
        PatrolDatum patrol = PatrolData[index]
        Enz04_PatrolCollectionScript collection = patrol.PatrolCollection as Enz04_PatrolCollectionScript
        If collection != None
            If collection.GetCount() > 0 && PatrolCenterMarkers != None && patrol.PatrolCenterMarker != None
                PatrolCenterMarkers.AddRef(patrol.PatrolCenterMarker)
            ElseIf collection.GetCount() == 0 && PatrolCenterMarkers != None && patrol.PatrolCenterMarker != None
                PatrolCenterMarkers.RemoveRef(patrol.PatrolCenterMarker)
            EndIf
            collection.CleanupPatrol(False)
        EndIf
        index += 1
    EndWhile
EndFunction

Event OnQuestShutdown()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    Int index = 0
    While PatrolData != None && index < PatrolData.Length
        CancelTimer(index)
        CancelTimer(index + iFailsafeIDOffset)
        index += 1
    EndWhile
EndEvent
