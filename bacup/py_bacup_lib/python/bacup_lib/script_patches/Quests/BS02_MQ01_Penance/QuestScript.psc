Function StartLocalWave(ObjectReference spawnCenter, Int completionStage)
    RefCollectionAlias localEnemies = GetAlias(28) as RefCollectionAlias
    GenericEWSModuleRef waveConfig = spawnCenter as GenericEWSModuleRef
    ActorBase enemyBase = Game.GetFormFromFile(0x00605C2D, "SeventySix.esm") as ActorBase
    Actor player = Alias_Player.GetActorReference()
    Int totalEnemyCount = GetRequestedLocalWaveCount(waveConfig)
    Int regularEnemyCount = totalEnemyCount
    Int spawnedEnemyCount = 0
    Int enemyIndex = 0
    Int existingEnemyCount
    Int existingEnemyIndex = 0

    If spawnCenter == None || localEnemies == None || enemyBase == None || player == None || totalEnemyCount <= 0
        Return
    EndIf
    If completionStage == 1500
        If !IsStageDone(1400) || IsStageDone(1500)
            Return
        EndIf
    ElseIf completionStage == 2300
        If !IsStageDone(2200) || IsStageDone(2300)
            Return
        EndIf
    Else
        Return
    EndIf
    existingEnemyCount = localEnemies.GetCount()
    While existingEnemyIndex < existingEnemyCount
        Actor existingEnemy = localEnemies.GetAt(existingEnemyIndex) as Actor
        If existingEnemy != None && existingEnemy.IsCreated() && existingEnemy.GetActorBase() == enemyBase && !existingEnemy.IsDead()
            Return
        EndIf
        existingEnemyIndex += 1
    EndWhile
    CleanupLocalWaveRefs(localEnemies, enemyBase)
    If waveConfig.BossWaveAtEnd
        regularEnemyCount -= 1
    EndIf
    While enemyIndex < totalEnemyCount
        Int levelModifier = 2
        Bool spawnCommitted = False
        If waveConfig.BossWaveAtEnd && enemyIndex == regularEnemyCount
            levelModifier = 3
        EndIf
        Actor enemyRef = spawnCenter.PlaceActorAtMe(enemyBase, levelModifier)
        If enemyRef != None
            localEnemies.AddRef(enemyRef)
            If localEnemies.Find(enemyRef) >= 0
                spawnedEnemyCount += 1
                spawnCommitted = True
            Else
                enemyRef.Disable()
                enemyRef.Delete()
            EndIf
        EndIf
        If !spawnCommitted
            enemyIndex = totalEnemyCount
        Else
            enemyIndex += 1
        EndIf
    EndWhile
    If spawnedEnemyCount != totalEnemyCount
        CleanupLocalWaveRefs(localEnemies, enemyBase)
        Return
    EndIf
    enemyIndex = 0
    existingEnemyCount = localEnemies.GetCount()
    While enemyIndex < existingEnemyCount
        Actor enemyRef = localEnemies.GetAt(enemyIndex) as Actor
        If enemyRef != None && enemyRef.IsCreated() && enemyRef.GetActorBase() == enemyBase
            RegisterForRemoteEvent(enemyRef, "OnDeath")
            enemyRef.StartCombat(player, True)
        EndIf
        enemyIndex += 1
    EndWhile
EndFunction

Function CompleteLocalWaveIfAllDead()
    RefCollectionAlias localEnemies = GetAlias(28) as RefCollectionAlias
    ActorBase enemyBase = Game.GetFormFromFile(0x00605C2D, "SeventySix.esm") as ActorBase
    ReferenceAlias spawnCenterAlias
    GenericEWSModuleRef waveConfig
    Int expectedEnemyCount
    Int localEnemyCount = 0
    Bool foundEnemy = False
    Int enemyCount
    Int enemyIndex = 0

    If localEnemies == None || enemyBase == None
        Return
    EndIf
    If IsStageDone(2200) && !IsStageDone(2300)
        spawnCenterAlias = GetAlias(37) as ReferenceAlias
    ElseIf IsStageDone(1400) && !IsStageDone(1500)
        spawnCenterAlias = GetAlias(27) as ReferenceAlias
    Else
        Return
    EndIf
    If spawnCenterAlias == None
        Return
    EndIf
    waveConfig = spawnCenterAlias.GetReference() as GenericEWSModuleRef
    expectedEnemyCount = GetRequestedLocalWaveCount(waveConfig)
    If expectedEnemyCount <= 0
        Return
    EndIf
    enemyCount = localEnemies.GetCount()
    If enemyCount <= 0
        Return
    EndIf
    While enemyIndex < enemyCount
        Actor enemyRef = localEnemies.GetAt(enemyIndex) as Actor
        If enemyRef != None && enemyRef.IsCreated() && enemyRef.GetActorBase() == enemyBase
            foundEnemy = True
            localEnemyCount += 1
            If !enemyRef.IsDead()
                Return
            EndIf
        EndIf
        enemyIndex += 1
    EndWhile
    If !foundEnemy
        Return
    EndIf
    If localEnemyCount != expectedEnemyCount
        Return
    EndIf
    CleanupLocalWaveRefs(localEnemies, enemyBase)
    If IsStageDone(2200) && !IsStageDone(2300)
        SetStage(2300)
    ElseIf IsStageDone(1400) && !IsStageDone(1500)
        SetStage(1500)
    EndIf
EndFunction

Int Function GetRequestedLocalWaveCount(GenericEWSModuleRef waveConfig)
    If waveConfig == None
        Return 0
    EndIf
    Int regularEnemiesPerRepeat = 1
    Int repeatCount = 1
    If waveConfig.ActorSkew > 0
        regularEnemiesPerRepeat = waveConfig.ActorSkew
    EndIf
    If waveConfig.RepeatType == 2 && waveConfig.RepeatNumber > 0
        repeatCount = waveConfig.RepeatNumber
    EndIf
    Int requestedEnemyCount = regularEnemiesPerRepeat * repeatCount
    If waveConfig.BossWaveAtEnd
        requestedEnemyCount += 1
    EndIf
    Return requestedEnemyCount
EndFunction

Function CleanupLocalWaveRefs(RefCollectionAlias localEnemies, ActorBase enemyBase)
    If localEnemies == None || enemyBase == None
        Return
    EndIf
    Int enemyIndex = localEnemies.GetCount() - 1
    While enemyIndex >= 0
        Actor enemyRef = localEnemies.GetAt(enemyIndex) as Actor
        If enemyRef != None && enemyRef.IsCreated() && enemyRef.GetActorBase() == enemyBase
            UnregisterForRemoteEvent(enemyRef, "OnDeath")
            localEnemies.RemoveRef(enemyRef)
            enemyRef.Disable()
            enemyRef.Delete()
        EndIf
        enemyIndex -= 1
    EndWhile
EndFunction

Function AttemptMissingHandoff(Quest missingQuest, Keyword startKeyword)
    Actor player = Alias_Player.GetActorReference()
    Bool accepted = False

    If missingQuest != None
        accepted = missingQuest.IsRunning() || missingQuest.IsCompleted()
    EndIf
    If !accepted && player != None && missingQuest != None && startKeyword != None
        accepted = startKeyword.SendStoryEventAndWait(None, player, player)
        If !accepted
            accepted = missingQuest.IsRunning() || missingQuest.IsCompleted()
        EndIf
    EndIf
    If accepted
        Stop()
    Else
        StartTimer(5.0, 9000)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 9000 || !IsStageDone(9000)
        Return
    EndIf
    Quest missingQuest = Game.GetFormFromFile(0x005F56C4, "SeventySix.esm") as Quest
    Keyword startKeyword = Game.GetFormFromFile(0x005F56B8, "SeventySix.esm") as Keyword
    AttemptMissingHandoff(missingQuest, startKeyword)
EndEvent

Function PrepareMineScene()
    Actor shin = Alias_Actor_Shin_CavernsInt.GetActorReference()
    ObjectReference mineSceneMarker = Alias_Marker_ShinMineScene.GetReference()
    Float waited = 0.0
    Float waitInterval = 0.25

    ShinReachedMineSceneMarker = False
    If shin == None || mineSceneMarker == None
        Return
    EndIf
    shin.EvaluatePackage()
    While shin.GetDistance(mineSceneMarker) > 128.0 && waited < ShinTeleportFallbackTime
        Utility.Wait(waitInterval)
        waited += waitInterval
    EndWhile
    If shin.GetDistance(mineSceneMarker) > 128.0
        shin.MoveTo(mineSceneMarker)
    EndIf
    If IdleMineExplosionReadyLoop != None
        shin.PlayIdle(IdleMineExplosionReadyLoop)
    EndIf
    ShinReachedMineSceneMarker = True
EndFunction

Function ResolveMineBlast()
    Actor shin = Alias_Actor_Shin_CavernsInt.GetActorReference()
    If shin != None && IdleMineExplosionReady_Knockdown != None
        shin.PlayIdle(IdleMineExplosionReady_Knockdown)
    EndIf
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    RefCollectionAlias localEnemies = GetAlias(28) as RefCollectionAlias
    If localEnemies != None && localEnemies.Find(akSender) >= 0
        CompleteLocalWaveIfAllDead()
    EndIf
EndEvent
