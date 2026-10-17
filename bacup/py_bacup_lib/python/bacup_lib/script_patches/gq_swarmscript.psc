; FO76's server rolled the swarm's creature, wave count and boss, then marched each wave on a
; defended spot. Tales' roaming pulse passes the creature, the spot and the wave count through
; B21BeginRoaming; a swarm started any other way configures itself near the player.
; Waves run one after another: each spawns at a marker beside the spot and the next follows a
; short pause after the last one dies. Objectives 10-50 and 100 are fixed in the record, while the
; bound swarmObjectiveIndex and bossObjectiveIndex are -1 placeholders the FO76 server filled.
Event OnQuestInit()
    StartTimer(10.0, 7600)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 7600 && myEncounterWaveData == None && IsRunning()
        Actor playerRef = Game.GetPlayer()
        ObjectReference anchor = B21:EncounterWaveCatalog.PlaceRoamingMarker(playerRef, 3000.0)
        If !B21BeginRoaming(None, anchor, playerRef.GetCurrentLocation(), Utility.RandomInt(1, 3), Utility.RandomInt(2, 4))
            If anchor != None
                anchor.Delete()
            EndIf
            Stop()
        EndIf
    ElseIf aiTimerID == HordeCheckTimerID && myEncounterWaveData != None
        B21StartNextSwarmWave()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(7600)
    CancelTimer(HordeCheckTimerID)
    If myEncounterWaveData != None
        UnregisterForCustomEvent(myEncounterWaveData, "WaveComplete")
    EndIf
    myEncounterWaveData = None
    currentWaveIndex = -1
    bossWaveIndex = -1
    subwaveTimerExpired = False
EndEvent

Event DefaultQuestEncounterWaveScript.WaveComplete(DefaultQuestEncounterWaveScript akSender, Var[] akArgs)
    If akSender != myEncounterWaveData || akArgs == None || akArgs.Length == 0 || (akArgs[0] as Int) != currentWaveIndex
        Return
    EndIf
    SetObjectiveCompleted(B21SwarmObjective(currentWaveIndex), True)
    If Waves != None && currentWaveIndex < Waves.Length && Waves[currentWaveIndex].rewardStage >= 0
        SetStage(Waves[currentWaveIndex].rewardStage)
    EndIf
    StartTimer(8.0, HordeCheckTimerID)
EndEvent

Bool Function B21BeginRoaming(Keyword akCreature, ObjectReference akAnchor, Location akRegion, Int aiBossRank, Int aiWaves)
    If myEncounterWaveData != None || akAnchor == None
        Return False
    EndIf
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript ews = owner as DefaultQuestEncounterWaveScript
    B21:EncounterWaveCatalog catalog = owner as B21:EncounterWaveCatalog
    If ews == None || catalog == None || ews.EncounterWaves == None || ews.EncounterWaves.Length < 2
        Return False
    EndIf
    Keyword creature = akCreature
    If catalog.KeywordVariantCount(creature) <= 0
        creature = catalog.RandomPoolKeyword()
    EndIf
    If creature == None
        Return False
    EndIf
    ReferenceAlias spawnStart = GetAlias(7) as ReferenceAlias
    ObjectReference spawnMarker = B21:EncounterWaveCatalog.PlaceRoamingMarker(akAnchor, 1500.0)
    If spawnStart == None || spawnMarker == None
        Return False
    EndIf
    myEncounterWaveData = ews

    Alias_SwarmCenterMarker.ForceRefTo(akAnchor)
    spawnStart.ForceRefTo(spawnMarker)
    akAnchor.Delete()
    spawnMarker.Delete()

    ; The last wave slot is kept for the boss, so at most EncounterWaves.Length - 1 regular waves.
    WaveCount = aiWaves
    If WaveCount < 1
        WaveCount = 1
    ElseIf WaveCount > ews.EncounterWaves.Length - 1
        WaveCount = ews.EncounterWaves.Length - 1
    EndIf
    bossWaveIndex = -1
    If allowBossWave && (debugOverrideAlwaysBossWave || Utility.RandomFloat(0.0, 1.0) < BossEpicChance)
        bossWaveIndex = WaveCount
    EndIf
    Int rank = aiBossRank
    If rank < 1
        rank = 1
    ElseIf rank > 5
        rank = 5
    EndIf
    Int waveIndex = 0
    While waveIndex < ews.EncounterWaves.Length
        DefaultQuestEncounterWaveScript:EncounterWaveData waveData = ews.EncounterWaves[waveIndex]
        waveData.EncounterTypeKeyword = creature
        waveData.SpawnArea = spawnStart
        waveData.Difficulty = waveIndex + 1
        If waveData.Difficulty > 4
            waveData.Difficulty = 4
        EndIf
        waveData.BossWave = waveIndex == bossWaveIndex
        waveData.BossWaveAllowLackeys = True
        waveData.ManuallyOverrideLegendaryCreatureGeneration = waveIndex == bossWaveIndex
        waveData.DesiredMinNumLegendaryCreatures = 1
        waveData.DesiredMaxNumLegendaryCreatures = 1
        waveData.MinLegendaryCreatureRank = rank
        waveData.MaxLegendaryCreatureRank = rank
        waveIndex += 1
    EndWhile

    RegisterForCustomEvent(ews, "WaveComplete")
    currentWaveIndex = -1
    progressPercentage = 0.0
    If moveToLocationStage >= 0
        SetStage(moveToLocationStage)
    EndIf
    B21StartNextSwarmWave()
    Return True
EndFunction

Int Function B21SwarmObjective(Int aiWaveIndex)
    If aiWaveIndex == bossWaveIndex
        Return 100
    EndIf
    Return (aiWaveIndex + 1) * 10
EndFunction

Function B21StartNextSwarmWave()
    Int lastWave = WaveCount - 1
    If bossWaveIndex >= 0
        lastWave = bossWaveIndex
    EndIf
    currentWaveIndex += 1
    If currentWaveIndex > lastWave
        progressPercentage = 1.0
        SetStage(completionStage)
        Return
    EndIf
    progressPercentage = (currentWaveIndex as Float) / ((lastWave + 1) as Float)

    ObjectReference center = Alias_SwarmCenterMarker.GetReference()
    ObjectReference spawnMarker = (GetAlias(7) as ReferenceAlias).GetReference()
    If center != None && spawnMarker != None && currentWaveIndex > 0
        Float angle = Utility.RandomFloat(0.0, 360.0)
        spawnMarker.MoveTo(center, 1500.0 * Math.Cos(angle), 1500.0 * Math.Sin(angle), 0.0, True)
        spawnMarker.MoveToNearestNavmeshLocation()
    EndIf

    DefaultQuestEncounterWaveScript:EncounterWaveData waveData = myEncounterWaveData.EncounterWaves[currentWaveIndex]
    myEncounterWaveData.StartEncounterWave(currentWaveIndex)
    If waveData.WaveRefCollection != None && waveData.WaveRefCollection.GetCount() > 0
        ObjectReference firstEnemy = waveData.WaveRefCollection.GetAt(0)
        If currentWaveIndex == 0
            Alias_SwarmName.ForceRefTo(firstEnemy)
        EndIf
        If currentWaveIndex == bossWaveIndex && Alias_BossName != None
            Alias_BossName.ForceRefTo(firstEnemy)
        EndIf
    EndIf
    ; Objectives point at the center until this is set, then at the wave's enemies.
    subwaveTimerExpired = True
    SetObjectiveDisplayed(B21SwarmObjective(currentWaveIndex), True)
    If currentWaveIndex == 0 && startAttackStage >= 0
        SetStage(startAttackStage)
    EndIf
EndFunction

ObjectReference Function B21RoamingAnchor()
    Return Alias_SwarmCenterMarker.GetReference()
EndFunction
