Event OnQuestInit()
    Parent.OnQuestInit()
    bCombatTargetTimerActive = False
    UpdateCombatTargets()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    Parent.OnStageSet(auiStageID, auiItemID)
    UpdateCombatTargets()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iCombatTargetTimerID
        bCombatTargetTimerActive = False
        UpdateCombatTargets()
    Else
        Parent.OnTimer(aiTimerID)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(iCombatTargetTimerID)
    bCombatTargetTimerActive = False
    Parent.OnQuestShutdown()
EndEvent

Function AddLivingActors(RefCollectionAlias akCollection, Actor[] akActors)
    Int i = 0
    While akCollection != None && i < akCollection.GetCount()
        Actor combatant = akCollection.GetAt(i) as Actor
        If combatant != None && combatant != Game.GetPlayer() && !combatant.IsDead() && !combatant.IsDisabled() && akActors.Find(combatant) < 0
            akActors.Add(combatant)
        EndIf
        i += 1
    EndWhile
EndFunction

Function UpdateCombatTargets()
    If IsStopping() || !IsRunning() || IsCompleted() || IsStageDone(175) || IsStageDone(180) || IsStageDone(190) || IsStageDone(iWrapupStage)
        CancelTimer(iCombatTargetTimerID)
        bCombatTargetTimerActive = False
        Return
    EndIf
    If !IsStageDone(60) || AttackBotsColl == None || AttackPlayerColl == None
        Return
    EndIf
    If !bCombatTargetTimerActive
        bCombatTargetTimerActive = True
        StartTimer(iCombatTargetUpdateTimer as Float, iCombatTargetTimerID)
    EndIf
    Actor[] patrol = new Actor[0]
    AddLivingActors(PatrolBots, patrol)
    If patrol.Length == 0
        Return
    EndIf
    Actor[] enemies = new Actor[0]
    Int waveIndex = 0
    While EncounterWaves != None && waveIndex < EncounterWaves.Length
        AddLivingActors(EncounterWaves[waveIndex].WaveRefCollection, enemies)
        AddLivingActors(EncounterWaves[waveIndex].WaveRefCollectionSecondary, enemies)
        AddLivingActors(EncounterWaves[waveIndex].BossRefCollection, enemies)
        waveIndex += 1
    EndWhile
    Float playerFraction = 0.5
    If ENz04_EnemiesTargetingPlayers != None
        playerFraction = ENz04_EnemiesTargetingPlayers.GetValue()
    EndIf
    If playerFraction < 0.0
        playerFraction = 0.0
    ElseIf playerFraction > 1.0
        playerFraction = 1.0
    EndIf
    Int botTargets = enemies.Length - ((enemies.Length * playerFraction) as Int)
    Int minimumBotTargets = 1
    If ENz04_MinimumBotTargetingEnemies != None
        minimumBotTargets = ENz04_MinimumBotTargetingEnemies.GetValueInt()
    EndIf
    If botTargets < minimumBotTargets
        botTargets = minimumBotTargets
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || playerRef.IsDead()
        botTargets = enemies.Length
    EndIf
    AttackBotsColl.RemoveAll()
    AttackPlayerColl.RemoveAll()
    Int i = 0
    While i < enemies.Length
        Actor targetActor
        If i < botTargets
            AttackBotsColl.AddRef(enemies[i])
            targetActor = patrol[i % patrol.Length]
        Else
            AttackPlayerColl.AddRef(enemies[i])
            targetActor = playerRef
        EndIf
        If targetActor != None && enemies[i].GetCombatTarget() != targetActor
            enemies[i].StartCombat(targetActor)
        EndIf
        i += 1
    EndWhile
EndFunction

; Wave-data accessors for ENz04_BotScript, which materializes the source
; regional pools locally. The struct stays private to this script family.
Bool Function ENz04_HasWave(Int aiWaveIndex)
    Return EncounterWaves != None && aiWaveIndex >= 0 && aiWaveIndex < EncounterWaves.Length
EndFunction

Bool Function ENz04_WaveHasBoss(Int aiWaveIndex)
    If !ENz04_HasWave(aiWaveIndex)
        Return False
    EndIf
    Return EncounterWaves[aiWaveIndex].BossRefCollection != None
EndFunction

Int Function ENz04_WaveEnemyCount(Int aiWaveIndex)
    If !ENz04_HasWave(aiWaveIndex)
        Return 0
    EndIf
    EncounterWaveData waveData = EncounterWaves[aiWaveIndex]
    If waveData.BossRefCollection == None
        Return 4
    EndIf
    ; The native compiler rejects returning a struct Int member directly; passing it works.
    Return ENz04_BossWaveCount(waveData.MaxWaveActorsOverride)
EndFunction

Int Function ENz04_BossWaveCount(Int aiOverride)
    If aiOverride > 0
        Return aiOverride
    EndIf
    Return 3
EndFunction

ObjectReference Function ENz04_WaveSpawnCenter(Int aiWaveIndex)
    If !ENz04_HasWave(aiWaveIndex)
        Return None
    EndIf
    ReferenceAlias spawnArea = EncounterWaves[aiWaveIndex].SpawnArea
    If spawnArea == None
        Return None
    EndIf
    Return spawnArea.GetReference()
EndFunction

Function ENz04_AddWaveActor(Int aiWaveIndex, Actor akEnemy, Bool abBoss)
    If akEnemy == None || !ENz04_HasWave(aiWaveIndex)
        Return
    EndIf
    EncounterWaveData waveData = EncounterWaves[aiWaveIndex]
    If abBoss && waveData.BossRefCollection != None
        waveData.BossRefCollection.AddRef(akEnemy)
    ElseIf waveData.WaveRefCollection != None
        waveData.WaveRefCollection.AddRef(akEnemy)
    EndIf
EndFunction
