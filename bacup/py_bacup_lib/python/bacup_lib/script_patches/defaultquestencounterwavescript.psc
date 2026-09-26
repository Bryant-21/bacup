Event OnQuestInit()
    QuestShuttingDown = False
    B21ActiveLocalWaves = New Int[0]
    B21UncollectedActors = None
    B21UncollectedSpawned = None
    B21UncollectedSpawning = False
    B21SpawningWaves = New Int[0]
    B21WaveActors = None
    B21WaveActorWaves = None
    B21WaveActorOwned = None
    B21WaveSubwaves = None
    B21WaveActorsSpawned = None
    B21WaveTimeRemaining = None
    B21WaveLegendariesSpawned = None
    B21WaveSpawning = False
    initialized = True
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    ReconcileLocalEncounterWaveRegistrations()
    ReconcileUncollectedScorched()
EndEvent

Event OnQuestShutdown()
    QuestShuttingDown = True
    CancelTimer(8876)
    ClearUncollectedScorched()
    StopAllEncounterWaves(True)
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
        ReconcileUncollectedScorched()
        ReconcileLocalEncounterWaveRegistrations()
        Int waveIndex = 0
        While EncounterWaves && waveIndex < EncounterWaves.Length
            ; Catalog-backed waves own their spawn and end bookkeeping, so collection-only replay must not end them early.
            If !WaveUsesCatalog(waveIndex)
                EncounterWaveData waveData = EncounterWaves[waveIndex]
                Bool spawnRecorded = (waveData.StageToSetOnFirstWaveSpawn >= 0 && IsStageDone(waveData.StageToSetOnFirstWaveSpawn)) || (waveData.StageToSetOnLastWaveSpawn >= 0 && IsStageDone(waveData.StageToSetOnLastWaveSpawn))
                If spawnRecorded && waveData.StageToSetAtEnd >= 0 && !IsStageDone(waveData.StageToSetAtEnd) && LocalWaveHasAnyReference(waveData)
                    ActivateLocalWave(waveIndex)
                EndIf
                EvaluateLocalEncounterWave(waveIndex, False)
            EndIf
            waveIndex += 1
        EndWhile
    EndIf
EndEvent

Function StartLocalEncounterWave(Int aiWaveIndex)
    If QuestShuttingDown || !EncounterWaves || aiWaveIndex < 0 || aiWaveIndex >= EncounterWaves.Length
        Return
    EndIf

    EncounterWaveData waveData = EncounterWaves[aiWaveIndex]
    ActivateLocalWave(aiWaveIndex)
    If waveData.WaveRefCollection == None && waveData.WaveRefCollectionSecondary == None && waveData.BossRefCollection == None
        ReconcileUncollectedScorched()
    EndIf
    RegisterLocalWaveCollection(waveData.WaveRefCollection, True)
    RegisterLocalWaveCollection(waveData.WaveRefCollectionSecondary, True)
    RegisterLocalWaveCollection(waveData.BossRefCollection, True)
    SetLocalEncounterStageIfPending(waveData.StageToSetOnFirstWaveSpawn)
    SetLocalEncounterStageIfPending(waveData.StageToSetOnLastWaveSpawn)
    EvaluateLocalEncounterWave(aiWaveIndex, True)
EndFunction

Function ActivateLocalWave(Int aiWaveIndex)
    If B21ActiveLocalWaves == None
        B21ActiveLocalWaves = New Int[0]
    EndIf
    Int index = B21ActiveLocalWaves.Length - 1
    While index >= 0
        Int previousWave = B21ActiveLocalWaves[index]
        If previousWave != aiWaveIndex && LocalWavesShareCollection(previousWave, aiWaveIndex)
            B21ActiveLocalWaves.Remove(index)
        EndIf
        index -= 1
    EndWhile
    If B21ActiveLocalWaves.Find(aiWaveIndex) < 0
        B21ActiveLocalWaves.Add(aiWaveIndex)
    EndIf
EndFunction

Bool Function LocalWavesShareCollection(Int aiFirst, Int aiSecond)
    EncounterWaveData firstWave = EncounterWaves[aiFirst]
    EncounterWaveData secondWave = EncounterWaves[aiSecond]
    Int index = 0
    While index < 3
        RefCollectionAlias collection
        If index == 0
            collection = firstWave.WaveRefCollection
        ElseIf index == 1
            collection = firstWave.WaveRefCollectionSecondary
        Else
            collection = firstWave.BossRefCollection
        EndIf
        If collection != None && (collection == secondWave.WaveRefCollection || collection == secondWave.WaveRefCollectionSecondary || collection == secondWave.BossRefCollection)
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Event OnStageSet(Int auiStageID, Int auiItemID)
    ReconcileUncollectedScorched()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 8876
        ReconcileUncollectedScorched()
    ElseIf aiTimerID >= 8900 && aiTimerID < 9000
        SpawnEncounterSubwave(aiTimerID - 8900)
    EndIf
EndEvent

Function ReconcileUncollectedScorched()
    Int waveIndex = -1
    Int startStage
    Int stopStage
    If Self == Game.GetFormFromFile(0x0003363B, "SeventySix.esm")
        waveIndex = 0
        startStage = 60
        stopStage = 70
    ElseIf Self == Game.GetFormFromFile(0x00045A40, "SeventySix.esm")
        waveIndex = 3
        startStage = 611
        stopStage = 700
    EndIf
    If waveIndex < 0 || QuestShuttingDown || !IsRunning() || B21UncollectedSpawning
        Return
    EndIf
    If GetStage() >= stopStage
        CancelTimer(8876)
        ClearUncollectedScorched()
        Return
    EndIf
    If !IsStageDone(startStage) || EncounterWaves == None || waveIndex >= EncounterWaves.Length
        Return
    EndIf
    ReferenceAlias markerAlias = EncounterWaves[waveIndex].SpawnArea
    ObjectReference marker
    If markerAlias != None
        marker = markerAlias.GetReference()
    EndIf
    Actor playerRef = Game.GetPlayer()
    If marker == None || playerRef == None
        StartTimer(5.0, 8876)
        Return
    EndIf
    B21UncollectedSpawning = True
    If B21UncollectedActors == None
        B21UncollectedActors = New Actor[3]
        B21UncollectedSpawned = New Bool[3]
    EndIf
    Int index = 0
    Bool retry = False
    While index < 3 && !QuestShuttingDown && GetStage() < stopStage
        Actor combatant = B21UncollectedActors[index]
        If !B21UncollectedSpawned[index]
            Int actorFormID = 0x0031B1FF
            If index == 0 || (index == 2 && Utility.RandomInt(0, 1) == 0)
                actorFormID = 0x0008E624
            EndIf
            If waveIndex == 0 && index == 0 && Utility.RandomInt(0, 1) == 0
                actorFormID = 0x0031B1FF
            ElseIf waveIndex == 0 && index == 1
                actorFormID = 0x0008E624
            EndIf
            Form spawnForm = Game.GetFormFromFile(actorFormID, "SeventySix.esm")
            If spawnForm != None
                combatant = marker.PlaceAtMe(spawnForm, 1, True, True, False) as Actor
            EndIf
            If QuestShuttingDown || GetStage() >= stopStage
                If combatant != None
                    combatant.DisableNoWait()
                    combatant.Delete()
                EndIf
                B21UncollectedSpawning = False
                Return
            EndIf
            If combatant != None
                B21UncollectedActors[index] = combatant
                B21UncollectedSpawned[index] = True
            Else
                retry = True
            EndIf
        EndIf
        If combatant != None && !combatant.IsDead()
            combatant.EnableNoWait()
            combatant.StartCombat(playerRef)
        EndIf
        index += 1
    EndWhile
    B21UncollectedSpawning = False
    If QuestShuttingDown || GetStage() >= stopStage
        ClearUncollectedScorched()
    ElseIf retry
        StartTimer(5.0, 8876)
    EndIf
EndFunction

Function ClearUncollectedScorched()
    If B21UncollectedActors == None
        Return
    EndIf
    Int index = 0
    While index < B21UncollectedActors.Length
        Actor combatant = B21UncollectedActors[index]
        If combatant != None && !combatant.IsDead()
            combatant.DisableNoWait()
            combatant.Delete()
        EndIf
        B21UncollectedActors[index] = None
        index += 1
    EndWhile
EndFunction

Function HandleLocalEncounterActorDeath(ObjectReference akSenderRef)
    If QuestShuttingDown || akSenderRef == None || !EncounterWaves
        Return
    EndIf

    Int waveIndex = 0
    While waveIndex < EncounterWaves.Length
        EncounterWaveData waveData = EncounterWaves[waveIndex]
        If B21ActiveLocalWaves != None && B21ActiveLocalWaves.Find(waveIndex) >= 0 && LocalWaveContainsReference(waveData, akSenderRef)
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

    If B21ActiveLocalWaves == None || B21ActiveLocalWaves.Find(aiWaveIndex) < 0
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
    ElseIf B21WaveActors != None && B21WaveActors.Find(akSender) >= 0
        HandleSpawnedWaveActorDeath(akSender)
    Else
        HandleLocalEncounterActorDeath(akSender)
    EndIf
EndEvent

Int Function FindEncounterWaveIndex(String asIDString)
    If !EncounterWaves || asIDString == ""
        Return -1
    EndIf
    Return EncounterWaves.FindStruct("IDString", asIDString)
EndFunction

Bool Function WaveUsesCatalog(Int aiWaveIndex)
    B21:EncounterWaveCatalog catalog = (Self as Quest) as B21:EncounterWaveCatalog
    Return catalog != None && catalog.VariantCount(aiWaveIndex) > 0
EndFunction

Var[] Function WaveEventArgs(Int aiWaveIndex)
    Var[] args = New Var[1]
    args[0] = aiWaveIndex
    Return args
EndFunction

Function StartEncounterWaveByID(String asIDString)
    StartEncounterWave(FindEncounterWaveIndex(asIDString))
EndFunction

Function StopEncounterWaveByID(String asIDString, Bool abRemoveActors = False)
    StopEncounterWave(FindEncounterWaveIndex(asIDString), abRemoveActors)
EndFunction

Function StartEncounterWave(Int aiWaveIndex)
    If QuestShuttingDown || !EncounterWaves || aiWaveIndex < 0 || aiWaveIndex >= EncounterWaves.Length
        Return
    EndIf
    ; FO76's EMS supplied these actors. Without converted WAVE rows the wave can only use references its collections already hold.
    If !WaveUsesCatalog(aiWaveIndex)
        StartLocalEncounterWave(aiWaveIndex)
        Return
    EndIf
    EnsureSpawnWaveState()
    If B21SpawningWaves.Find(aiWaveIndex) >= 0
        Return
    EndIf

    EncounterWaveData waveData = EncounterWaves[aiWaveIndex]
    B21SpawningWaves.Add(aiWaveIndex)
    B21WaveSubwaves[aiWaveIndex] = 0
    B21WaveActorsSpawned[aiWaveIndex] = 0
    B21WaveLegendariesSpawned[aiWaveIndex] = 0
    B21WaveTimeRemaining[aiWaveIndex] = waveData.WaveTimeSeconds
    IncludeNearbyWaveActors(aiWaveIndex)
    SpawnEncounterSubwave(aiWaveIndex)
EndFunction

Function StopEncounterWave(Int aiWaveIndex, Bool abRemoveActors = False)
    If !EncounterWaves || aiWaveIndex < 0 || aiWaveIndex >= EncounterWaves.Length
        Return
    EndIf
    CancelTimer(8900 + aiWaveIndex)
    If B21SpawningWaves != None
        Int spawningIndex = B21SpawningWaves.Find(aiWaveIndex)
        If spawningIndex >= 0
            B21SpawningWaves.Remove(spawningIndex)
        EndIf
    EndIf
    If abRemoveActors
        RemoveWaveActors(aiWaveIndex)
    Else
        ; A stopped wave is complete, so its remaining attackers can still finish it.
        EvaluateSpawnedWaveEnd(aiWaveIndex)
    EndIf
EndFunction

Function StopAllEncounterWaves(Bool abRemoveActors = False)
    Int waveIndex = 0
    While EncounterWaves && waveIndex < EncounterWaves.Length
        StopEncounterWave(waveIndex, abRemoveActors)
        waveIndex += 1
    EndWhile
EndFunction

Bool Function IsEncounterWaveSpawning(Int aiWaveIndex)
    Return B21SpawningWaves != None && B21SpawningWaves.Find(aiWaveIndex) >= 0
EndFunction

Function EnsureSpawnWaveState()
    If B21SpawningWaves == None
        B21SpawningWaves = New Int[0]
    EndIf
    Int waveCount = EncounterWaves.Length
    If B21WaveSubwaves == None || B21WaveSubwaves.Length != waveCount
        B21WaveSubwaves = New Int[waveCount]
        B21WaveActorsSpawned = New Int[waveCount]
        B21WaveTimeRemaining = New Float[waveCount]
        B21WaveLegendariesSpawned = New Int[waveCount]
    EndIf
EndFunction

Float Function SubwaveIntervalSeconds(EncounterWaveData akWave)
    ; CT_EWS_SubwaveTime_* curves (the Short variants hold the same values).
    Float seconds = 60.0
    If akWave.SubwaveTimer_Speed == 0
        seconds = 120.0
    ElseIf akWave.SubwaveTimer_Speed == 1
        seconds = 90.0
    ElseIf akWave.SubwaveTimer_Speed == 3
        seconds = 30.0
    ElseIf akWave.SubwaveTimer_Speed == 4
        seconds = 15.0
    ElseIf akWave.SubwaveTimer_Speed == 5
        seconds = 5.0
    ElseIf akWave.SubwaveTimer_Speed == 6
        seconds = 0.0
    EndIf
    If akWave.SubwaveDelayMult > 0.0
        seconds *= akWave.SubwaveDelayMult
    EndIf
    If seconds < 1.0
        seconds = 1.0
    EndIf
    Return seconds
EndFunction

Int Function SubwaveActorCount(EncounterWaveData akWave)
    If akWave.BossWave
        If akWave.BossWaveAllowLackeys
            Return 1 + Utility.RandomInt(0, 2)
        EndIf
        Return 1
    EndIf
    Int count = akWave.Difficulty + 1
    If akWave.Difficulty < 0 || akWave.Difficulty > 4
        count = 3
    EndIf
    If akWave.MaxSubwaveActorsOverride > 0 && count > akWave.MaxSubwaveActorsOverride
        count = akWave.MaxSubwaveActorsOverride
    EndIf
    If akWave.MinSubwaveActorsOverride > 1 && count < akWave.MinSubwaveActorsOverride
        count = akWave.MinSubwaveActorsOverride
    EndIf
    Return count
EndFunction

Int Function WaveLiveActorCap(EncounterWaveData akWave)
    If akWave.MaxWaveActorsOverride > 0
        Return akWave.MaxWaveActorsOverride
    EndIf
    Return 5
EndFunction

Int Function WaveDurationLimit(EncounterWaveData akWave)
    Int limit = akWave.WaveDuration
    If limit < 1
        limit = 1
    ElseIf limit > 99
        limit = 99
    EndIf
    Return limit
EndFunction

ObjectReference Function ResolveWaveSpawnPoint(EncounterWaveData akWave)
    If akWave.SpawnArea != None && akWave.SpawnArea.GetReference() != None
        Return akWave.SpawnArea.GetReference()
    EndIf
    Quest owner = Self as Quest
    DefaultEventQuest eventQuest = owner as DefaultEventQuest
    If eventQuest != None && eventQuest.CenterMarker != None
        Return eventQuest.CenterMarker.GetReference()
    EndIf
    Return None
EndFunction

Function SpawnEncounterSubwave(Int aiWaveIndex)
    If QuestShuttingDown || !IsEncounterWaveSpawning(aiWaveIndex)
        Return
    EndIf
    If B21WaveSpawning
        StartTimer(1.0, 8900 + aiWaveIndex)
        Return
    EndIf
    B21WaveSpawning = True

    EncounterWaveData waveData = EncounterWaves[aiWaveIndex]
    B21:EncounterWaveCatalog catalog = (Self as Quest) as B21:EncounterWaveCatalog
    ObjectReference spawnPoint = ResolveWaveSpawnPoint(waveData)
    Int variants = 0
    If catalog != None
        variants = catalog.VariantCount(aiWaveIndex)
    EndIf
    Int spawnCount = SubwaveActorCount(waveData)
    Int capacity = WaveLiveActorCap(waveData) - CountWaveActorsTowardCap(aiWaveIndex, waveData)
    If !waveData.BossWave && spawnCount > capacity
        spawnCount = capacity
    EndIf
    If waveData.WaveType == 3 && !waveData.BossWave && spawnCount > WaveDurationLimit(waveData) - B21WaveActorsSpawned[aiWaveIndex]
        spawnCount = WaveDurationLimit(waveData) - B21WaveActorsSpawned[aiWaveIndex]
    EndIf

    Int spawned = 0
    If spawnPoint != None && variants > 0 && spawnCount > 0
        Int variant = Utility.RandomInt(0, variants - 1)
        Int actorIndex = 0
        While actorIndex < spawnCount && !QuestShuttingDown && IsEncounterWaveSpawning(aiWaveIndex)
            Form spawnForm = catalog.PickCandidate(aiWaveIndex, variant, actorIndex)
            Actor waveActor = None
            If spawnForm != None
                waveActor = spawnPoint.PlaceAtMe(spawnForm, 1, False, True, False) as Actor
            EndIf
            If waveActor != None
                If QuestShuttingDown || !IsEncounterWaveSpawning(aiWaveIndex)
                    waveActor.Delete()
                Else
                    AddWaveActor(aiWaveIndex, waveData, waveActor, True, waveData.BossWave && spawned == 0)
                    spawned += 1
                EndIf
            EndIf
            actorIndex += 1
        EndWhile
    EndIf
    B21WaveSpawning = False
    If QuestShuttingDown || !IsEncounterWaveSpawning(aiWaveIndex)
        Return
    EndIf

    B21WaveActorsSpawned[aiWaveIndex] = B21WaveActorsSpawned[aiWaveIndex] + spawned
    B21WaveSubwaves[aiWaveIndex] = B21WaveSubwaves[aiWaveIndex] + 1
    If B21WaveSubwaves[aiWaveIndex] == 1
        SetLocalEncounterStageIfPending(waveData.StageToSetOnFirstWaveSpawn)
        SendCustomEvent("FirstSubwaveSpawned", WaveEventArgs(aiWaveIndex))
    EndIf
    ScheduleNextSubwave(aiWaveIndex, waveData)
EndFunction

Function ScheduleNextSubwave(Int aiWaveIndex, EncounterWaveData akWave)
    Float interval = SubwaveIntervalSeconds(akWave)
    Bool finished = False
    If akWave.BossWave
        finished = True
    ElseIf akWave.WaveType == 1
        B21WaveTimeRemaining[aiWaveIndex] = B21WaveTimeRemaining[aiWaveIndex] - interval
        finished = B21WaveTimeRemaining[aiWaveIndex] <= 0.0
    ElseIf akWave.WaveType == 2
        finished = B21WaveSubwaves[aiWaveIndex] >= WaveDurationLimit(akWave)
    ElseIf akWave.WaveType == 3
        finished = B21WaveActorsSpawned[aiWaveIndex] >= WaveDurationLimit(akWave)
    ElseIf akWave.WaveType != 0
        finished = True
    EndIf

    If !finished
        StartTimer(interval, 8900 + aiWaveIndex)
        Return
    EndIf
    SetLocalEncounterStageIfPending(akWave.StageToSetOnLastWaveSpawn)
    SendCustomEvent("LastWave", WaveEventArgs(aiWaveIndex))
    StopEncounterWave(aiWaveIndex, False)
EndFunction

Function IncludeNearbyWaveActors(Int aiWaveIndex)
    EncounterWaveData waveData = EncounterWaves[aiWaveIndex]
    If waveData.IncludeRefCollAliasActors != None
        Int collectionIndex = 0
        While collectionIndex < waveData.IncludeRefCollAliasActors.GetCount()
            IncludeExistingWaveActor(aiWaveIndex, waveData, waveData.IncludeRefCollAliasActors.GetAt(collectionIndex) as Actor)
            collectionIndex += 1
        EndWhile
    EndIf
    If !waveData.IncludeOtherNearbyActors || waveData.FilterKeywords == None
        Return
    EndIf
    ObjectReference spawnPoint = ResolveWaveSpawnPoint(waveData)
    If spawnPoint == None
        Return
    EndIf
    Float radius = waveData.SpawnAreaRadiusMax
    If radius <= 0.0
        radius = 5120.0
    EndIf
    ObjectReference[] nearby = spawnPoint.FindAllReferencesWithKeyword(waveData.FilterKeywords, radius)
    Int nearbyIndex = 0
    While nearby != None && nearbyIndex < nearby.Length
        Actor nearbyActor = nearby[nearbyIndex] as Actor
        If nearbyActor != None && NearbyActorMatchesFilters(waveData, nearbyActor)
            IncludeExistingWaveActor(aiWaveIndex, waveData, nearbyActor)
        EndIf
        nearbyIndex += 1
    EndWhile
EndFunction

Bool Function NearbyActorMatchesFilters(EncounterWaveData akWave, Actor akActor)
    If akWave.AdditionalExclusionKeyword != None && akActor.HasKeyword(akWave.AdditionalExclusionKeyword)
        Return False
    EndIf
    If !akWave.RequireAllFilterKeywords
        Return True
    EndIf
    Int keywordIndex = 0
    While keywordIndex < akWave.FilterKeywords.GetSize()
        Keyword filterKeyword = akWave.FilterKeywords.GetAt(keywordIndex) as Keyword
        If filterKeyword != None && !akActor.HasKeyword(filterKeyword)
            Return False
        EndIf
        keywordIndex += 1
    EndWhile
    Return True
EndFunction

Function IncludeExistingWaveActor(Int aiWaveIndex, EncounterWaveData akWave, Actor akActor)
    If akActor == None || akActor.IsDead() || akActor == Game.GetPlayer()
        Return
    EndIf
    If B21WaveActors != None && B21WaveActors.Find(akActor) >= 0
        Return
    EndIf
    AddWaveActor(aiWaveIndex, akWave, akActor, False, False)
EndFunction

Function AddWaveActor(Int aiWaveIndex, EncounterWaveData akWave, Actor akActor, Bool abOwned, Bool abBoss)
    If B21WaveActors == None
        B21WaveActors = New Actor[0]
        B21WaveActorWaves = New Int[0]
        B21WaveActorOwned = New Bool[0]
    EndIf
    B21WaveActors.Add(akActor)
    B21WaveActorWaves.Add(aiWaveIndex)
    B21WaveActorOwned.Add(abOwned)

    If akWave.WaveRefCollection != None
        akWave.WaveRefCollection.AddRef(akActor)
    EndIf
    If akWave.WaveRefCollectionSecondary != None
        akWave.WaveRefCollectionSecondary.AddRef(akActor)
    EndIf
    If abBoss && akWave.BossRefCollection != None
        akWave.BossRefCollection.AddRef(akActor)
    EndIf
    If abOwned
        ApplyWaveLegendaryRank(aiWaveIndex, akWave, akActor)
    EndIf
    RegisterForRemoteEvent(akActor, "OnDeath")
    akActor.EnableNoWait()

    Bool targeted = ApplyWaveCollectionTargets(akWave.WaveRefCollection, akActor)
    If ApplyWaveCollectionTargets(akWave.WaveRefCollectionSecondary, akActor)
        targeted = True
    EndIf
    Actor playerRef = Game.GetPlayer()
    If !targeted && playerRef != None && akActor.GetDistance(playerRef) <= 4096.0
        akActor.StartCombat(playerRef)
    EndIf
EndFunction

Bool Function ApplyWaveCollectionTargets(RefCollectionAlias akCollection, Actor akActor)
    Quests:_Default:SetPreferredCombatTargets preferred = akCollection as Quests:_Default:SetPreferredCombatTargets
    If preferred == None
        Return False
    EndIf
    Return preferred.ApplyPreferredCombatTarget(akActor)
EndFunction

Function ApplyWaveLegendaryRank(Int aiWaveIndex, EncounterWaveData akWave, Actor akActor)
    If !akWave.ManuallyOverrideLegendaryCreatureGeneration || akWave.DesiredMaxNumLegendaryCreatures <= 0
        Return
    EndIf
    Int alreadySpawned = B21WaveLegendariesSpawned[aiWaveIndex]
    If alreadySpawned >= akWave.DesiredMaxNumLegendaryCreatures
        Return
    EndIf
    ; The minimum is guaranteed first; extra legendaries up to the maximum are a coin flip.
    If alreadySpawned >= akWave.DesiredMinNumLegendaryCreatures && Utility.RandomInt(0, 1) == 0
        Return
    EndIf
    If !Game.IsPluginInstalled("B21_TalesFromAppalachia.esm")
        Return
    EndIf
    ActorValue rankValue = Game.GetFormFromFile(0x00FFD809, "B21_TalesFromAppalachia.esm") as ActorValue
    If rankValue == None
        Return
    EndIf
    Int minRank = akWave.MinLegendaryCreatureRank
    If minRank < 1
        minRank = 1
    EndIf
    Int maxRank = akWave.MaxLegendaryCreatureRank
    If maxRank < minRank
        maxRank = minRank
    EndIf
    If maxRank > 5
        maxRank = 5
    EndIf
    akActor.SetValue(rankValue, Utility.RandomInt(minRank, maxRank) as Float)
    B21WaveLegendariesSpawned[aiWaveIndex] = alreadySpawned + 1
EndFunction

Int Function CountLivingWaveActors(Int aiWaveIndex)
    Int living = 0
    Int index = 0
    While B21WaveActors != None && index < B21WaveActors.Length
        Actor waveActor = B21WaveActors[index]
        If waveActor != None && B21WaveActorWaves[index] == aiWaveIndex && !waveActor.IsDead() && !waveActor.IsDisabled()
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

Int Function CountWaveActorsTowardCap(Int aiWaveIndex, EncounterWaveData akWave)
    Int living = 0
    Int index = 0
    While B21WaveActors != None && index < B21WaveActors.Length
        Actor waveActor = B21WaveActors[index]
        If waveActor != None && B21WaveActorWaves[index] == aiWaveIndex && !waveActor.IsDead() && !waveActor.IsDisabled()
            If B21WaveActorOwned[index] || !akWave.NearbyActorsIgnoreMaxSpawnCount
                living += 1
            EndIf
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

Function HandleSpawnedWaveActorDeath(Actor akActor)
    UnregisterForRemoteEvent(akActor, "OnDeath")
    Int index = B21WaveActors.Find(akActor)
    If index < 0
        Return
    EndIf
    Int waveIndex = B21WaveActorWaves[index]
    EncounterWaveData waveData = EncounterWaves[waveIndex]
    If waveData.RemoveActorsFromRefCollectionsOnDeath
        If waveData.WaveRefCollection != None
            waveData.WaveRefCollection.RemoveRef(akActor)
        EndIf
        If waveData.WaveRefCollectionSecondary != None
            waveData.WaveRefCollectionSecondary.RemoveRef(akActor)
        EndIf
        If waveData.BossRefCollection != None
            waveData.BossRefCollection.RemoveRef(akActor)
        EndIf
    EndIf
    ; Delete leaves the corpse lootable until its cell unloads; dropping the row keeps endless waves from growing the arrays.
    If B21WaveActorOwned[index]
        akActor.Delete()
    EndIf
    RemoveWaveActorRow(index)
    If !QuestShuttingDown
        EvaluateSpawnedWaveEnd(waveIndex)
    EndIf
EndFunction

Function RemoveWaveActorRow(Int aiRow)
    B21WaveActors.Remove(aiRow)
    B21WaveActorWaves.Remove(aiRow)
    B21WaveActorOwned.Remove(aiRow)
EndFunction

Function EvaluateSpawnedWaveEnd(Int aiWaveIndex)
    If QuestShuttingDown || IsEncounterWaveSpawning(aiWaveIndex) || B21WaveSubwaves == None || B21WaveSubwaves[aiWaveIndex] <= 0
        Return
    EndIf
    EncounterWaveData waveData = EncounterWaves[aiWaveIndex]
    Int living = CountLivingWaveActors(aiWaveIndex)
    If waveData.StageToSetOnXAttackersRemaining >= 0 && living <= waveData.XAttackersRemaining
        SetLocalEncounterStageIfPending(waveData.StageToSetOnXAttackersRemaining)
    EndIf
    If living <= 0
        ; Clearing the subwave count makes WaveComplete fire once per started wave.
        B21WaveSubwaves[aiWaveIndex] = 0
        SetLocalEncounterStageIfPending(waveData.StageToSetAtEnd)
        SendCustomEvent("WaveComplete", WaveEventArgs(aiWaveIndex))
    EndIf
EndFunction

Function RemoveWaveActors(Int aiWaveIndex)
    Int index = 0
    While B21WaveActors != None && index < B21WaveActors.Length
        Actor waveActor = B21WaveActors[index]
        If aiWaveIndex < 0 || B21WaveActorWaves[index] == aiWaveIndex
            If waveActor != None
                UnregisterForRemoteEvent(waveActor, "OnDeath")
                If B21WaveActorOwned[index]
                    If !waveActor.IsDead()
                        waveActor.DisableNoWait()
                    EndIf
                    waveActor.Delete()
                EndIf
            EndIf
            RemoveWaveActorRow(index)
        Else
            index += 1
        EndIf
    EndWhile
EndFunction
