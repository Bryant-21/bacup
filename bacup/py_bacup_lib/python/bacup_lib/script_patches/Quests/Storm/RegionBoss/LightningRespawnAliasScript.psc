Event OnAliasInit()
    ResetRespawnState()
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    QueueRespawn(akSenderRef)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != iRespawnTimerID
        Return
    EndIf
    If !RespawnAllowed()
        waitingForRespawn = New ObjectReference[0]
        iCurrentRespawnTargets = 0
        Return
    EndIf
    While waitingForRespawn != None && waitingForRespawn.Length > 0
        ObjectReference deadRef = waitingForRespawn[0]
        waitingForRespawn.Remove(0)
        RespawnFrom(deadRef)
    EndWhile
    iCurrentRespawnTargets = 0
EndEvent

Event OnAliasShutdown()
    CancelTimer(iRespawnTimerID)
    Int index = 0
    While B21RespawnedActors != None && index < B21RespawnedActors.Length
        Actor respawned = B21RespawnedActors[index]
        If respawned != None
            If !respawned.IsDead()
                respawned.DisableNoWait()
            EndIf
            respawned.Delete()
        EndIf
        index += 1
    EndWhile
    ResetRespawnState()
EndEvent

Function ResetRespawnState()
    waitingForRespawn = New ObjectReference[0]
    B21RespawnedActors = New Actor[0]
    iCurrentRespawnTargets = 0
    questScript = GetOwningQuest() as Quests:Storm:RegionBoss:RegionBossQuestScript
EndFunction

Bool Function RespawnAllowed()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning() || owningQuest.IsStageDone(iQuestCleanupStage)
        Return False
    EndIf
    Return questScript == None || !questScript.EventResolved()
EndFunction

Function QueueRespawn(ObjectReference akDeadRef)
    If akDeadRef == None || !RespawnAllowed()
        Return
    EndIf
    If waitingForRespawn == None
        waitingForRespawn = New ObjectReference[0]
    EndIf
    If waitingForRespawn.Find(akDeadRef) >= 0 || waitingForRespawn.Length >= iMaxRespawnTargets
        Return
    EndIf
    waitingForRespawn.Add(akDeadRef)
    iCurrentRespawnTargets = waitingForRespawn.Length
    If iCurrentRespawnTargets == 1
        StartTimer(fTimeToCheckForRespawn, iRespawnTimerID)
    EndIf
EndFunction

Function RespawnFrom(ObjectReference akDeadRef)
    Actor deadActor = akDeadRef as Actor
    If deadActor == None
        Return
    EndIf
    RemoveRef(deadActor)
    If B21RespawnedActors == None
        B21RespawnedActors = New Actor[0]
    EndIf
    ActorBase respawnBase = deadActor.GetActorBase()
    Bool canRespawn = respawnBase != None && deadActor.Is3DLoaded()
    Actor respawned = None
    If canRespawn
        respawned = deadActor.PlaceActorAtMe(respawnBase)
    EndIf
    Int ownedIndex = B21RespawnedActors.Find(deadActor)
    If ownedIndex >= 0
        ; Only corpses this script placed are ours to delete; wave-owned corpses are left to the wave script.
        B21RespawnedActors.Remove(ownedIndex)
        deadActor.Delete()
    EndIf
    If respawned == None
        Return
    EndIf
    B21RespawnedActors.Add(respawned)
    If bAllowRestrike
        AddRef(respawned)
    ElseIf MobCollectionAlias != None
        MobCollectionAlias.AddRef(respawned)
    EndIf
EndFunction
