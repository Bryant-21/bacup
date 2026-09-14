Event OnQuestInit()
    QuestShuttingDown = False
    initialized = True
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    ReconcileLocalEncounterWaveRegistrations()
EndEvent

Event OnQuestShutdown()
    QuestShuttingDown = True
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf

    Int waveIndex = 0
    While EncounterWaves && waveIndex < EncounterWaves.Length
        EncounterWaveData waveData = EncounterWaves[waveIndex]
        UnregisterLocalWaveCollection(waveData.WaveRefCollection)
        UnregisterLocalWaveCollection(waveData.WaveRefCollectionSecondary)
        UnregisterLocalWaveCollection(waveData.BossRefCollection)
        waveIndex += 1
    EndWhile
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer() && !QuestShuttingDown
        ReconcileLocalEncounterWaveRegistrations()
        Int waveIndex = 0
        While EncounterWaves && waveIndex < EncounterWaves.Length
            EvaluateLocalEncounterWave(waveIndex, False)
            waveIndex += 1
        EndWhile
    EndIf
EndEvent

Function StartLocalEncounterWave(Int aiWaveIndex)
    If QuestShuttingDown || !EncounterWaves || aiWaveIndex < 0 || aiWaveIndex >= EncounterWaves.Length
        Return
    EndIf

    EncounterWaveData waveData = EncounterWaves[aiWaveIndex]
    RegisterLocalWaveCollection(waveData.WaveRefCollection, True)
    RegisterLocalWaveCollection(waveData.WaveRefCollectionSecondary, True)
    RegisterLocalWaveCollection(waveData.BossRefCollection, True)
    SetLocalEncounterStageIfPending(waveData.StageToSetOnFirstWaveSpawn)
    SetLocalEncounterStageIfPending(waveData.StageToSetOnLastWaveSpawn)
    EvaluateLocalEncounterWave(aiWaveIndex, True)
EndFunction

Function HandleLocalEncounterActorDeath(ObjectReference akSenderRef)
    If QuestShuttingDown || akSenderRef == None || !EncounterWaves
        Return
    EndIf

    Int waveIndex = 0
    While waveIndex < EncounterWaves.Length
        EncounterWaveData waveData = EncounterWaves[waveIndex]
        If LocalWaveContainsReference(waveData, akSenderRef)
            EvaluateLocalEncounterWave(waveIndex, False, akSenderRef)
            Return
        EndIf
        waveIndex += 1
    EndWhile
EndFunction

Function ReconcileLocalEncounterWaveRegistrations()
    If QuestShuttingDown || !EncounterWaves
        Return
    EndIf

    Int waveIndex = 0
    While waveIndex < EncounterWaves.Length
        EncounterWaveData waveData = EncounterWaves[waveIndex]
        RegisterLocalWaveCollection(waveData.WaveRefCollection, False)
        RegisterLocalWaveCollection(waveData.WaveRefCollectionSecondary, False)
        RegisterLocalWaveCollection(waveData.BossRefCollection, False)
        waveIndex += 1
    EndWhile
EndFunction

Function RegisterLocalWaveCollection(RefCollectionAlias waveCollection, Bool abStartCombat)
    If waveCollection == None
        Return
    EndIf

    Actor playerRef = Game.GetPlayer()
    Int actorIndex = 0
    While actorIndex < waveCollection.GetCount()
        Actor waveActor = waveCollection.GetAt(actorIndex) as Actor
        If waveActor != None
            If abStartCombat
                waveActor.Enable()
            EndIf
            If !waveActor.IsDead()
                RegisterForRemoteEvent(waveActor, "OnDeath")
                If abStartCombat && playerRef != None
                    waveActor.StartCombat(playerRef)
                EndIf
            EndIf
        EndIf
        actorIndex += 1
    EndWhile
EndFunction

Function UnregisterLocalWaveCollection(RefCollectionAlias waveCollection)
    If waveCollection == None
        Return
    EndIf

    Int actorIndex = 0
    While actorIndex < waveCollection.GetCount()
        Actor waveActor = waveCollection.GetAt(actorIndex) as Actor
        If waveActor != None
            UnregisterForRemoteEvent(waveActor, "OnDeath")
        EndIf
        actorIndex += 1
    EndWhile
EndFunction

Bool Function LocalWaveContainsReference(EncounterWaveData waveData, ObjectReference targetRef)
    If targetRef == None
        Return False
    EndIf
    If waveData.WaveRefCollection != None && waveData.WaveRefCollection.Find(targetRef) >= 0
        Return True
    EndIf
    If waveData.WaveRefCollectionSecondary != None && waveData.WaveRefCollectionSecondary.Find(targetRef) >= 0
        Return True
    EndIf
    If waveData.BossRefCollection != None && waveData.BossRefCollection.Find(targetRef) >= 0
        Return True
    EndIf
    Return False
EndFunction

Bool Function LocalWaveHasAnyReference(EncounterWaveData waveData)
    If waveData.WaveRefCollection != None && waveData.WaveRefCollection.GetCount() > 0
        Return True
    EndIf
    If waveData.WaveRefCollectionSecondary != None && waveData.WaveRefCollectionSecondary.GetCount() > 0
        Return True
    EndIf
    If waveData.BossRefCollection != None && waveData.BossRefCollection.GetCount() > 0
        Return True
    EndIf
    Return False
EndFunction

Int Function CountLivingLocalWaveActors(EncounterWaveData waveData, ObjectReference akExcludedRef = None)
    Int livingCount = 0
    Int collectionIndex = 0
    While collectionIndex < 3
        RefCollectionAlias waveCollection
        If collectionIndex == 0
            waveCollection = waveData.WaveRefCollection
        ElseIf collectionIndex == 1
            waveCollection = waveData.WaveRefCollectionSecondary
        Else
            waveCollection = waveData.BossRefCollection
        EndIf

        If waveCollection != None
            Int actorIndex = 0
            While actorIndex < waveCollection.GetCount()
                Actor waveActor = waveCollection.GetAt(actorIndex) as Actor
                If waveActor != None && waveActor != akExcludedRef && !waveActor.IsDead()
                    livingCount += 1
                EndIf
                actorIndex += 1
            EndWhile
        EndIf
        collectionIndex += 1
    EndWhile
    Return livingCount
EndFunction

Int Function CountLivingCollectionActors(RefCollectionAlias waveCollection, ObjectReference akExcludedRef = None)
    If waveCollection == None
        Return 0
    EndIf

    Int livingCount = 0
    Int actorIndex = 0
    While actorIndex < waveCollection.GetCount()
        Actor waveActor = waveCollection.GetAt(actorIndex) as Actor
        If waveActor != None && waveActor != akExcludedRef && !waveActor.IsDead()
            livingCount += 1
        EndIf
        actorIndex += 1
    EndWhile
    Return livingCount
EndFunction

Function EvaluateLocalEncounterWave(Int aiWaveIndex, Bool abAllowEmpty, ObjectReference akExcludedRef = None)
    If QuestShuttingDown || !EncounterWaves || aiWaveIndex < 0 || aiWaveIndex >= EncounterWaves.Length
        Return
    EndIf

    EncounterWaveData waveData = EncounterWaves[aiWaveIndex]
    ; FO4 resolves Int-return helper calls with this nested struct parameter as None, so count the three bound collections locally.
    Int livingCount = 0
    Int collectionIndex = 0
    While collectionIndex < 3
        RefCollectionAlias waveCollection
        If collectionIndex == 0
            waveCollection = waveData.WaveRefCollection
        ElseIf collectionIndex == 1
            waveCollection = waveData.WaveRefCollectionSecondary
        Else
            waveCollection = waveData.BossRefCollection
        EndIf

        If waveCollection != None
            Int actorIndex = 0
            While actorIndex < waveCollection.GetCount()
                Actor waveActor = waveCollection.GetAt(actorIndex) as Actor
                If waveActor != None && waveActor != akExcludedRef && !waveActor.IsDead()
                    livingCount += 1
                EndIf
                actorIndex += 1
            EndWhile
        EndIf
        collectionIndex += 1
    EndWhile
    If waveData.StageToSetOnXAttackersRemaining >= 0 && livingCount <= waveData.XAttackersRemaining
        SetLocalEncounterStageIfPending(waveData.StageToSetOnXAttackersRemaining)
    EndIf

    If livingCount <= 0 && (abAllowEmpty || LocalWaveHasAnyReference(waveData))
        ; FO76's EMS can supply actors at runtime. FO4 cannot, so an explicitly started empty wave must fail forward instead of deadlocking the quest.
        SetLocalEncounterStageIfPending(waveData.StageToSetAtEnd)
    EndIf
EndFunction

Function SetLocalEncounterStageIfPending(Int aiStage)
    If aiStage >= 0 && !IsStageDone(aiStage)
        SetStage(aiStage)
    EndIf
EndFunction

Function StartBS01FieldTestingLocalEncounter()
    ; This existing server-only counter is unused by the FO4 substitute. A negative value scopes the shared OnDeath event to this BS01-only path.
    TotalNumLegendaryCreaturesToSpawn = -1
    RefCollectionAlias localEnemies = GetAlias(31) as RefCollectionAlias
    Actor enemyRef
    Int enemyCount
    Int enemyIndex = 0

    If localEnemies == None
        Return
    EndIf
    enemyCount = localEnemies.GetCount()
    While enemyIndex < enemyCount
        enemyRef = localEnemies.GetAt(enemyIndex) as Actor
        If enemyRef != None
            enemyRef.Enable()
            If !enemyRef.IsDead()
                RegisterForRemoteEvent(enemyRef, "OnDeath")
            EndIf
        EndIf
        enemyIndex += 1
    EndWhile
    CompleteBS01FieldTestingLocalEncounterIfAllDead()
EndFunction

Function CompleteBS01FieldTestingLocalEncounterIfAllDead(Actor akConfirmedDeath = None)
    RefCollectionAlias localEnemies = GetAlias(31) as RefCollectionAlias
    Actor enemyRef
    Bool foundEnemy = False
    Int enemyCount
    Int enemyIndex = 0

    If localEnemies == None
        Return
    EndIf
    enemyCount = localEnemies.GetCount()
    If enemyCount <= 0
        If akConfirmedDeath == None
            Return
        EndIf
    Else
        While enemyIndex < enemyCount
            enemyRef = localEnemies.GetAt(enemyIndex) as Actor
            If enemyRef != None
                foundEnemy = True
                If !enemyRef.IsDead()
                    Return
                EndIf
            EndIf
            enemyIndex += 1
        EndWhile
        If !foundEnemy && akConfirmedDeath == None
            Return
        EndIf
        enemyIndex = 0
        While enemyIndex < enemyCount
            enemyRef = localEnemies.GetAt(enemyIndex) as Actor
            If enemyRef != None
                UnregisterForRemoteEvent(enemyRef, "OnDeath")
            EndIf
            enemyIndex += 1
        EndWhile
    EndIf
    If akConfirmedDeath != None
        UnregisterForRemoteEvent(akConfirmedDeath, "OnDeath")
    EndIf
    TotalNumLegendaryCreaturesToSpawn = 0
    If !IsStageDone(980)
        SetStage(980)
    EndIf
    If !IsStageDone(990)
        SetStage(990)
    EndIf
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If TotalNumLegendaryCreaturesToSpawn < 0
        CompleteBS01FieldTestingLocalEncounterIfAllDead(akSender)
    Else
        HandleLocalEncounterActorDeath(akSender)
    EndIf
EndEvent
