; Bots on Parade controller. FO76 built the patrol, ran the defense timers and
; requested regional encounter waves from server services. The FO4 substitute
; keeps the retained stage constants and timer IDs, spawns the six patrol bots
; from the bound leveled bases at the site's spawn markers, and materializes
; each hostile wave from the source WAVE records of the site's location
; encounter properties before handing it to ENz04_EncounterWaveScript.
; Local timer IDs 6 (bot drop), 7 (orientation), 9 (event expiry) and 10
; (wrap-up) extend the retained 3/4/5.

Event OnQuestInit()
    B21StartedWaves = new Int[0]
    B21SpawnedEnemies = new Actor[0]
    B21SiteSpecies = -1
    bAudioCooldown = False
    iAssaultronsSpawned = 0
    iSentryBotsSpawned = 0
    iTotalBotsSpawned = 0
    Parent.OnQuestInit()
EndEvent

Event OnQuestShutdown()
    ENz04_CancelEventTimers()
    CancelTimer(10)
    ENz04_ReleaseUnloadedEnemies()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iBotDefenseTimerID
        If ENz04_EventActive() && !IsStageDone(iBotsReadyStage)
            SetStage(iBotsReadyStage)
        EndIf
    ElseIf aiTimerID == iHostilesTimerID
        If ENz04_EventActive() && !IsStageDone(iTriggerHostilesStage)
            SetStage(iTriggerHostilesStage)
        EndIf
    ElseIf aiTimerID == iWarningCooldownID
        bAudioCooldown = False
    ElseIf aiTimerID == 6
        ENz04_DropBots()
    ElseIf aiTimerID == 7
        If ENz04_EventActive() && !IsStageDone(iBeginDefenseStage)
            SetStage(iBeginDefenseStage)
        EndIf
    ElseIf aiTimerID == 9
        If ENz04_EventActive() && !IsStageDone(175)
            SetStage(175)
        EndIf
    ElseIf aiTimerID == 10
        If IsRunning() && !IsStageDone(200)
            SetStage(200)
        EndIf
    EndIf
EndEvent

Bool Function ENz04_EventActive()
    Return IsRunning() && !IsStopping() && !IsStageDone(175) && !IsStageDone(180) && !IsStageDone(190)
EndFunction

Function ENz04_BeginStartup()
    Float eventSeconds = 0.0
    If EventTimeLong != None
        eventSeconds = EventTimeLong.GetValue()
    EndIf
    If eventSeconds > 0.0
        StartTimer(eventSeconds, 9)
    EndIf
EndFunction

Function ENz04_BeginConstruction()
    If !ENz04_EventActive() || (PatrolBots != None && PatrolBots.GetCount() > 0)
        Return
    EndIf
    If ENz04_Bots_0010_InitiatingConstruction != None
        ENz04_Bots_0010_InitiatingConstruction.Start()
    EndIf
    ENz04_SetConstructionSounds(True)
    Int tubeIndex = 0
    While SpawnTubes != None && tubeIndex < SpawnTubes.GetCount() && iTotalBotsSpawned < iBotThreshold
        ObjectReference tube = SpawnTubes.GetAt(tubeIndex)
        ObjectReference spawnMarker = ENz04_NearestReference(BotSpawns, tube)
        If tube != None && spawnMarker != None
            Actor bot = spawnMarker.PlaceAtMe(ENz04_PickBotBase(), 1, False, True, False) as Actor
            If bot != None
                PatrolBots.AddRef(bot)
                iTotalBotsSpawned += 1
                B21TwoStateActivator76 tubeDoor = tube as B21TwoStateActivator76
                If tubeDoor != None
                    tubeDoor.SetOpenNoWait(True)
                EndIf
            EndIf
        EndIf
        tubeIndex += 1
    EndWhile
    StartTimer(iBotDropTimerLength as Float, 6)
EndFunction

ActorBase Function ENz04_PickBotBase()
    If iAssaultronsSpawned < iAssaultronLimit && Utility.RandomInt(1, 100) <= iAssaultronChance
        iAssaultronsSpawned += 1
        Return ENz04_LvlAssaultron
    EndIf
    If iSentryBotsSpawned < iSentryBotLimit
        iSentryBotsSpawned += 1
        Return ENz04_LvlSentryBot
    EndIf
    Return Enz04_LvlMrGutsy
EndFunction

Function ENz04_DropBots()
    If !ENz04_EventActive()
        Return
    EndIf
    Int index = 0
    While PatrolBots != None && index < PatrolBots.GetCount()
        Actor bot = PatrolBots.GetAt(index) as Actor
        If bot != None
            bot.Enable(True)
            bot.SetUnconscious(True)
        EndIf
        index += 1
    EndWhile
    StartTimer(iOrientationTimerLength as Float, 7)
EndFunction

Function ENz04_BeginDefense()
    If !ENz04_EventActive()
        Return
    EndIf
    ENz04_SetConstructionSounds(False)
    StartTimer(ENz04_PowerUpSeconds(), iBotDefenseTimerID)
    StartTimer(iHostileTimerSpawnLength as Float, iHostilesTimerID)
EndFunction

; The source objective timer global ENz04_BotPowerUpTimer wins over the retained
; 180-second skeleton default.
Float Function ENz04_PowerUpSeconds()
    GlobalVariable powerUpTimer = Game.GetFormFromFile(0x00394346, "SeventySix.esm") as GlobalVariable
    If powerUpTimer != None && powerUpTimer.GetValue() > 0.0
        Return powerUpTimer.GetValue()
    EndIf
    Return iBotDefenseTimerLength as Float
EndFunction

Function ENz04_BotsOnline()
    CancelTimer(iBotDefenseTimerID)
    Int index = 0
    While PatrolBots != None && index < PatrolBots.GetCount()
        Actor bot = PatrolBots.GetAt(index) as Actor
        If bot != None && !bot.IsDead()
            bot.SetUnconscious(False)
            bot.EvaluatePackage()
        EndIf
        index += 1
    EndWhile
EndFunction

Function ENz04_StepOutPatrol()
    If ENz04_Bots_0070_ReprogrammingComplete != None
        ENz04_Bots_0070_ReprogrammingComplete.Start()
    EndIf
    Int index = 0
    While PatrolBots != None && index < PatrolBots.GetCount()
        Actor bot = PatrolBots.GetAt(index) as Actor
        If bot != None && !bot.IsDead()
            ObjectReference spawnMarker = ENz04_NearestReference(BotSpawns, bot)
            ObjectReference stepOut = None
            If spawnMarker != None
                stepOut = spawnMarker.GetLinkedRef(ENz04_StepOutLinkKeyword)
            EndIf
            If stepOut != None
                bot.SetLinkedRef(stepOut, ENz04_StepOutLinkKeyword)
            EndIf
            bot.EvaluatePackage()
        EndIf
        index += 1
    EndWhile
    Int tubeIndex = 0
    While SpawnTubes != None && tubeIndex < SpawnTubes.GetCount()
        B21TwoStateActivator76 tubeDoor = SpawnTubes.GetAt(tubeIndex) as B21TwoStateActivator76
        If tubeDoor != None
            tubeDoor.SetOpenNoWait(False)
        EndIf
        tubeIndex += 1
    EndWhile
EndFunction

Function ENz04_AnnouncePatrol()
    Int index = 0
    While PatrolBots != None && index < PatrolBots.GetCount()
        Actor bot = PatrolBots.GetAt(index) as Actor
        If bot != None && !bot.IsDead()
            If ENz04_InitiatingPatrol != None
                bot.Say(ENz04_InitiatingPatrol)
            EndIf
            Return
        EndIf
        index += 1
    EndWhile
EndFunction

Function ENz04_BeginWrapUp()
    ENz04_CancelEventTimers()
    StartTimer(60.0, 10)
EndFunction

Function ENz04_CancelEventTimers()
    CancelTimer(iBotDefenseTimerID)
    CancelTimer(iHostilesTimerID)
    CancelTimer(iWarningCooldownID)
    CancelTimer(6)
    CancelTimer(7)
    CancelTimer(9)
EndFunction

Function ENz04_StartHostileWave(Int aiWaveIndex)
    If !ENz04_EventActive() || IsStageDone(2)
        Return
    EndIf
    If B21StartedWaves == None
        B21StartedWaves = new Int[0]
    EndIf
    If B21StartedWaves.Find(aiWaveIndex) >= 0
        Return
    EndIf
    ENz04_EncounterWaveScript waves = (Self as Quest) as ENz04_EncounterWaveScript
    If waves == None || !waves.ENz04_HasWave(aiWaveIndex)
        Return
    EndIf
    B21StartedWaves.Add(aiWaveIndex)
    ObjectReference spawnCenter = waves.ENz04_WaveSpawnCenter(aiWaveIndex)
    If spawnCenter == None && AttackCenterMarker != None
        spawnCenter = AttackCenterMarker.GetReference()
    EndIf
    If spawnCenter != None
        Bool bossWave = waves.ENz04_WaveHasBoss(aiWaveIndex)
        Int enemyCount = waves.ENz04_WaveEnemyCount(aiWaveIndex)
        Int enemyIndex = 0
        While enemyIndex < enemyCount && ENz04_EventActive()
            Bool bossSlot = bossWave && enemyIndex == 0
            Actor enemy = B21:EnclaveEventSupport.PlaceOnRing(spawnCenter, B21:EnclaveEventSupport.SpeciesForm(ENz04_SiteSpecies(), bossSlot))
            If enemy != None
                waves.ENz04_AddWaveActor(aiWaveIndex, enemy, bossSlot)
                If B21SpawnedEnemies == None
                    B21SpawnedEnemies = new Actor[0]
                EndIf
                B21SpawnedEnemies.Add(enemy)
            EndIf
            enemyIndex += 1
        EndWhile
    EndIf
    waves.StartLocalEncounterWave(aiWaveIndex)
EndFunction

; Species codes follow the source ESSChanceMain weights of each site location:
; 0 super mutants, 1 scorched, 2 mole miners, 3 feral ghouls, 4 vicious dogs,
; 5 molerats, 6 rad rats, 7 liberators, 8 robots, 9 bloatflies, 10 cave crickets.
Int Function ENz04_SiteSpecies()
    If B21SiteSpecies >= 0
        Return B21SiteSpecies
    EndIf
    ObjectReference center = None
    If CenterMarkerActual != None
        center = CenterMarkerActual.GetReference()
    EndIf
    Int species = 1
    If B21:EnclaveEventSupport.CenterInLocation(center, 0x0012B7C4)
        species = 0
    ElseIf B21:EnclaveEventSupport.CenterInLocation(center, 0x00012F6B)
        species = B21:EnclaveEventSupport.PickOne(2, 1)
    ElseIf B21:EnclaveEventSupport.CenterInLocation(center, 0x00188B78)
        Int roll = Utility.RandomInt(1, 75)
        If roll <= 5
            species = 3
        ElseIf roll <= 10
            species = 4
        ElseIf roll <= 20
            species = 5
        ElseIf roll <= 25
            species = 6
        ElseIf roll <= 40
            species = 2
        ElseIf roll <= 45
            species = 7
        ElseIf roll <= 50
            species = 0
        ElseIf roll <= 55
            species = 8
        ElseIf roll <= 65
            species = 1
        ElseIf roll <= 70
            species = 9
        Else
            species = 10
        EndIf
    EndIf
    B21SiteSpecies = species
    Return species
EndFunction

ObjectReference Function ENz04_NearestReference(RefCollectionAlias akCollection, ObjectReference akOrigin)
    ObjectReference nearest = None
    Float nearestDistance = 0.0
    Int index = 0
    While akCollection != None && akOrigin != None && index < akCollection.GetCount()
        ObjectReference candidate = akCollection.GetAt(index)
        If candidate != None
            Float distance = akOrigin.GetDistance(candidate)
            If nearest == None || distance < nearestDistance
                nearest = candidate
                nearestDistance = distance
            EndIf
        EndIf
        index += 1
    EndWhile
    Return nearest
EndFunction

Function ENz04_SetConstructionSounds(Bool abOn)
    Int index = 0
    While ConstructionMarkers != None && index < ConstructionMarkers.GetCount()
        ObjectReference soundMarker = ConstructionMarkers.GetAt(index)
        If soundMarker != None
            If abOn
                ObjectReference tube = None
                If SpawnTubes != None && index < SpawnTubes.GetCount()
                    tube = SpawnTubes.GetAt(index)
                EndIf
                If tube != None
                    soundMarker.MoveTo(tube)
                EndIf
                soundMarker.EnableNoWait()
            Else
                soundMarker.DisableNoWait()
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function ENz04_ReleaseUnloadedEnemies()
    B21:EnclaveEventSupport.ReleaseUnloadedActors(B21SpawnedEnemies)
    B21SpawnedEnemies = None
EndFunction
