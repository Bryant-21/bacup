; A Real Blast controller. FO76 ran the lure state machine, the regional
; encounter waves, the orbital strike and the quest timers on the server. The
; retained timer IDs are kept: iHordeTimerOffset spaces the six waves,
; iOrientationTimerOffset schedules lure malfunctions and iFleeTimerID is the
; retreat window. Local IDs: 20000 event expiry (QuestExpireGlobal_Event),
; 20001 wrap-up before Stop, 20002 cargobot fly-off failsafe, 20003 the
; all-enemies watch that runs once every wave has spawned.

Event OnQuestInit()
    B21SpawnedEnemies = new Actor[0]
    B21VertibotReleased = False
    iLureIndex = 0
    iLuresOriented = 0
    iWavesCleared = 0
    iOrientationsQueued = 0
    iLureOrientationsShown = 0
    bFirstLureActivatedOnce = False
    bTriggerOrientationLine = False
    bProcessingLure = False
    bProcessingOrientation = False
    Parent.OnQuestInit()
EndEvent

Event OnQuestShutdown()
    ENs02_Shutdown()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 60 || auiStageID == 65 || auiStageID == 70 || auiStageID == 75 || auiStageID == 80 || auiStageID == 85
        iWavesCleared += 1
        ENs02_CheckEnemiesCleared()
    ElseIf auiStageID == iAllEnemiesSpawnedStage
        ENs02_CheckOrientationComplete()
        ENs02_CheckEnemiesCleared()
        StartTimer(5.0, 20003)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iHordeTimerOffset
        If ENs02_EventActive() && iLureIndex < 6
            ENs02_StartWave(iLureIndex)
        EndIf
    ElseIf aiTimerID == iOrientationTimerOffset
        If ENs02_EventActive() && !IsStageDone(iAllEnemiesSpawnedStage)
            ENs02_BreakLure()
            StartTimer(iOrienationTimerLength as Float, iOrientationTimerOffset)
        EndIf
    ElseIf aiTimerID == iFleeTimerID
        If ENs02_EventActive() && !IsStageDone(123)
            SetStage(123)
        EndIf
    ElseIf aiTimerID == 20000
        If ENs02_EventActive()
            SetStage(160)
        EndIf
    ElseIf aiTimerID == 20001
        If IsRunning()
            SetStage(200)
        EndIf
    ElseIf aiTimerID == 20002
        ENs02_DismissVertibot()
    ElseIf aiTimerID == 20003
        If ENs02_EventActive() && !IsStageDone(iWipeOutRemainingEnemiesStage) && !IsStageDone(iEnemiesDeadStage)
            ENs02_CheckEnemiesCleared()
            StartTimer(5.0, 20003)
        EndIf
    EndIf
EndEvent

Event ObjectReference.OnTranslationComplete(ObjectReference akSender)
    If Vertibot != None && akSender == Vertibot.GetReference()
        ENs02_DismissVertibot()
    EndIf
EndEvent

Bool Function ENs02_EventActive()
    Return IsRunning() && !IsStopping() && !IsStageDone(iCompletionStage) && !IsStageDone(160)
EndFunction

; Stage 10. The six lure array sections fill alias Lures by ref type; the
; cargobot's source dish leads the order so wave one attacks it first.
Function ENs02_BeginStartup()
    ObjectReference sourceDish = None
    ReferenceAlias sourceAlias = GetAlias(50) as ReferenceAlias
    If sourceAlias != None
        sourceDish = sourceAlias.GetReference()
    EndIf
    ObjectReference[] ordered = new ObjectReference[0]
    If sourceDish != None && Lures != None && Lures.Find(sourceDish) >= 0
        ordered.Add(sourceDish)
    EndIf
    Int index = 0
    While Lures != None && index < Lures.GetCount()
        ObjectReference lure = Lures.GetAt(index)
        If lure != None && ordered.Find(lure) < 0
            ordered.Add(lure)
        EndIf
        index += 1
    EndWhile
    If LuresObj != None
        LuresObj.RemoveAll()
    EndIf
    If LuresToOrient != None
        LuresToOrient.RemoveAll()
    EndIf
    If PlayerOrientedLure != None
        PlayerOrientedLure.RemoveAll()
    EndIf
    index = 0
    While index < ordered.Length
        ObjectReference lure = ordered[index]
        If LuresObj != None
            LuresObj.AddRef(lure)
        EndIf
        If LureAliases != None && index < LureAliases.Length && LureAliases[index] != None
            LureAliases[index].ForceRefTo(lure)
        EndIf
        ObjectReference spawnCenter = lure.GetLinkedRef(None)
        If SpawnCenterAliases != None && index < SpawnCenterAliases.Length && SpawnCenterAliases[index] != None && spawnCenter != None
            SpawnCenterAliases[index].ForceRefTo(spawnCenter)
        EndIf
        ENs02_SetLureState(lure, 0)
        index += 1
    EndWhile
    ObjectReference cargobot = ENs02_VertibotRef()
    If cargobot != None
        cargobot.EnableNoWait()
    EndIf
    GlobalVariable expireLength = Game.GetFormFromFile(0x0038093D, "SeventySix.esm") as GlobalVariable
    If expireLength != None && expireLength.GetValue() > 0.0
        StartTimer(expireLength.GetValue(), 20000)
    EndIf
EndFunction

; Alias LuresObj forwards player activation of an uninitialized lure here.
Function ENs02_LureInitialized(ObjectReference akLure)
    If akLure == None || !ENs02_EventActive() || IsStageDone(iLuresEnabledStage) || LuresObj == None || LuresObj.Find(akLure) < 0
        Return
    EndIf
    LuresObj.RemoveRef(akLure)
    ENs02_LureRefScript lureScript = akLure as ENs02_LureRefScript
    If lureScript != None
        lureScript.BroadcastInitSound()
    EndIf
    ENs02_SetLureState(akLure, 1)
    If !bFirstLureActivatedOnce
        bFirstLureActivatedOnce = True
        ENEvent_SayToPlayer(ENs02_FirstLurePlanted)
    EndIf
    Utility.Wait(2.0)
    If ENs02_EventActive()
        ENs02_SetLureState(akLure, 2)
    EndIf
    If LuresObj.GetCount() == 0 && ENs02_EventActive() && !IsStageDone(iLuresEnabledStage)
        SetStage(iLuresEnabledStage)
    EndIf
EndFunction

; Stage 50. The lures sync with the orbital platform while the first horde
; and the first malfunction are scheduled.
Function ENs02_BeginDefense()
    ENEvent_SayToPlayer(ENs02_AllLuresPlaced)
    ObjectReference syncMarker = None
    If SyncSoundMarker != None
        syncMarker = SyncSoundMarker.GetReference()
    EndIf
    If syncMarker != None
        syncMarker.EnableNoWait()
    EndIf
    ENs02_BlastMarkerScript blast = ENs02_BlastMarkerRef() as ENs02_BlastMarkerScript
    If blast != None
        blast.BroadcastArrayActiveAudio_Impl()
    EndIf
    iLureIndex = 0
    StartTimer(iHordeTimerLength as Float, iHordeTimerOffset)
    StartTimer(Utility.RandomInt(iInitialOrientationTimerMin, iInitialOrientationTimerMax) as Float, iOrientationTimerOffset)
EndFunction

Function ENs02_StartWave(Int aiWaveIndex)
    If !ENs02_EventActive() || aiWaveIndex < 0 || aiWaveIndex > 5
        Return
    EndIf
    iLureIndex = aiWaveIndex + 1
    DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
    RefCollectionAlias waveEnemies = GetAlias(ENs02_WaveCollectionID(aiWaveIndex)) as RefCollectionAlias
    ObjectReference spawnCenter = None
    If SpawnCenterAliases != None && aiWaveIndex < SpawnCenterAliases.Length && SpawnCenterAliases[aiWaveIndex] != None
        spawnCenter = SpawnCenterAliases[aiWaveIndex].GetReference()
    EndIf
    If spawnCenter == None && LureAliases != None && aiWaveIndex < LureAliases.Length && LureAliases[aiWaveIndex] != None
        spawnCenter = LureAliases[aiWaveIndex].GetReference()
    EndIf
    If spawnCenter == None && CenterMarkerActual != None
        spawnCenter = CenterMarkerActual.GetReference()
    EndIf
    Int species = ENs02_SiteSpecies()
    Int enemyIndex = 0
    While spawnCenter != None && waveEnemies != None && enemyIndex < 4 && ENs02_EventActive()
        Actor enemy = B21:EnclaveEventSupport.PlaceOnRing(spawnCenter, B21:EnclaveEventSupport.SpeciesForm(species, False))
        If enemy != None
            waveEnemies.AddRef(enemy)
            If AllEnemies != None
                AllEnemies.AddRef(enemy)
            EndIf
            If B21SpawnedEnemies == None
                B21SpawnedEnemies = new Actor[0]
            EndIf
            B21SpawnedEnemies.Add(enemy)
        EndIf
        enemyIndex += 1
    EndWhile
    If aiWaveIndex == 0
        ENEvent_SayToPlayer(ENs02_HostileDetected)
    EndIf
    If waves != None
        waves.StartLocalEncounterWave(aiWaveIndex)
    EndIf
EndFunction

; Spawn stages 58/63/68/73/78/83.
Function ENs02_WaveSpawned(Int aiWaveIndex)
    RefCollectionAlias waveEnemies = GetAlias(ENs02_WaveCollectionID(aiWaveIndex)) as RefCollectionAlias
    Int index = 0
    While waveEnemies != None && AllEnemies != None && index < waveEnemies.GetCount()
        ObjectReference enemy = waveEnemies.GetAt(index)
        If enemy != None && AllEnemies.Find(enemy) < 0
            AllEnemies.AddRef(enemy)
        EndIf
        index += 1
    EndWhile
    If aiWaveIndex >= 5
        If !IsStageDone(iAllEnemiesSpawnedStage)
            SetStage(iAllEnemiesSpawnedStage)
        EndIf
    ElseIf ENs02_EventActive()
        iLureIndex = aiWaveIndex + 1
        StartTimer(iHordeTimerLength as Float, iHordeTimerOffset)
    EndIf
EndFunction

Int Function ENs02_WaveCollectionID(Int aiWaveIndex)
    If aiWaveIndex == 0
        Return 19
    ElseIf aiWaveIndex == 1
        Return 20
    ElseIf aiWaveIndex == 2
        Return 21
    ElseIf aiWaveIndex == 3
        Return 22
    ElseIf aiWaveIndex == 4
        Return 23
    EndIf
    Return 36
EndFunction

; Bound EncounterTypeKeyword is LocEncMain_Current, so each site's own
; ESSChanceMain pool applies: Bog Town super mutants, Red Rocket and
; Clarksburg robots, Seneca Rocks scorched, Mount Blair and Welch mole miners.
Int Function ENs02_SiteSpecies()
    ObjectReference center = None
    If CenterMarkerActual != None
        center = CenterMarkerActual.GetReference()
    EndIf
    If B21:EnclaveEventSupport.CenterInLocation(center, 0x0009A031) || B21:EnclaveEventSupport.CenterInLocation(center, 0x0005CC3C)
        Return 8
    ElseIf B21:EnclaveEventSupport.CenterInLocation(center, 0x0009A0D1)
        Return 1
    ElseIf B21:EnclaveEventSupport.CenterInLocation(center, 0x00093D5F) || B21:EnclaveEventSupport.CenterInLocation(center, 0x000B4871)
        Return 2
    EndIf
    Return 0
EndFunction

; Orientation timer. A working lure develops one of the four bound statuses
; and joins LuresToOrient until a player re-orients it.
Function ENs02_BreakLure()
    If Lures == None || LuresToOrient == None || DishStatusCollections == None || DishStatusCollections.Length == 0
        Return
    EndIf
    ObjectReference[] candidates = new ObjectReference[0]
    Int index = 0
    While index < Lures.GetCount()
        ObjectReference lure = Lures.GetAt(index)
        DefaultMultiStateActivator dish = lure as DefaultMultiStateActivator
        If dish != None && dish.CurrentStateIndex == 2 && LuresToOrient.Find(lure) < 0
            candidates.Add(lure)
        EndIf
        index += 1
    EndWhile
    If candidates.Length == 0
        Return
    EndIf
    ObjectReference target = candidates[Utility.RandomInt(0, candidates.Length - 1)]
    ENs02_DishStatusScript status = DishStatusCollections[Utility.RandomInt(0, DishStatusCollections.Length - 1)] as ENs02_DishStatusScript
    If status == None
        Return
    EndIf
    LuresToOrient.AddRef(target)
    iOrientationsQueued += 1
    status.ENs02_ApplyStatus(target)
    iLureOrientationsShown += 1
    If !IsStageDone(iFirstOrientationStage)
        SetStage(iFirstOrientationStage)
    EndIf
    ENEvent_SayToPlayer(ENs02_OrientationRequired)
EndFunction

; Alias LuresToOrient forwards player activation of a malfunctioning lure here.
Function ENs02_LureOriented(ObjectReference akLure)
    If akLure == None || !ENs02_EventActive() || LuresToOrient == None || LuresToOrient.Find(akLure) < 0
        Return
    EndIf
    LuresToOrient.RemoveRef(akLure)
    Int index = 0
    While DishStatusCollections != None && index < DishStatusCollections.Length
        ENs02_DishStatusScript status = DishStatusCollections[index] as ENs02_DishStatusScript
        If status != None && status.Find(akLure) >= 0
            status.ENs02_ClearStatus(akLure)
        EndIf
        index += 1
    EndWhile
    ENs02_LureRefScript lureScript = akLure as ENs02_LureRefScript
    If lureScript != None
        lureScript.BroadcastSoundtoPlay()
    EndIf
    ENs02_SetLureState(akLure, 2)
    If PlayerOrientedLure != None
        PlayerOrientedLure.RemoveAll()
        PlayerOrientedLure.AddRef(akLure)
    EndIf
    iLuresOriented += 1
    ENEvent_SayToPlayer(ENs02_OrientationDetected)
    ENs02_CheckOrientationComplete()
EndFunction

; Stage 100 needs every wave spawned and no lure awaiting orientation.
Function ENs02_CheckOrientationComplete()
    If !ENs02_EventActive() || IsStageDone(iOrientationCompleteStage) || !IsStageDone(iAllEnemiesSpawnedStage)
        Return
    EndIf
    If LuresToOrient != None && LuresToOrient.GetCount() > 0
        Return
    EndIf
    SetStage(iOrientationCompleteStage)
EndFunction

; Stage 145 before the strike means the players cleared the field early.
Function ENs02_CheckEnemiesCleared()
    If !ENs02_EventActive() || !IsStageDone(iAllEnemiesSpawnedStage) || IsStageDone(iEnemiesDeadStage) || IsStageDone(iWipeOutRemainingEnemiesStage)
        Return
    EndIf
    If ENs02_LivingEnemies() == 0
        SetStage(iEnemiesDeadStage)
    EndIf
EndFunction

Int Function ENs02_LivingEnemies()
    Int living = 0
    Int index = 0
    While AllEnemies != None && index < AllEnemies.GetCount()
        Actor enemy = AllEnemies.GetAt(index) as Actor
        If enemy != None && !enemy.IsDead()
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

; Stage 100.
Function ENs02_WarnFlee()
    ObjectReference syncMarker = None
    If SyncSoundMarker != None
        syncMarker = SyncSoundMarker.GetReference()
    EndIf
    If syncMarker != None
        syncMarker.DisableNoWait()
    EndIf
    CancelTimer(iOrientationTimerOffset)
    ENEvent_SayToPlayer(ENs02_FleeAreaImmediately)
    If !IsStageDone(110)
        SetStage(110)
    EndIf
EndFunction

; Stage 110. Objective 100's bound timer global has no FO4 objective timer;
; the retained iFleeTimerLength drives the retreat window instead.
Function ENs02_StartFleeTimer()
    StartTimer(iFleeTimerLength as Float, iFleeTimerID)
EndFunction

; Stage 123. The shooter supplies the entry sound and air burst; its missile
; is fired level from a thousand units up, so the ground detonation at the
; blast marker is placed here with the bound explosion.
Function ENs02_LaunchStrike()
    ObjectReference blast = ENs02_BlastMarkerRef()
    If blast == None && CenterMarkerActual != None
        blast = CenterMarkerActual.GetReference()
    EndIf
    If blast == None
        If !IsStageDone(iWipeOutRemainingEnemiesStage)
            SetStage(iWipeOutRemainingEnemiesStage)
        EndIf
        Return
    EndIf
    ENs02_BlastMarkerScript blastScript = blast as ENs02_BlastMarkerScript
    If blastScript != None
        blastScript.BroadcastBombDropAudio_Impl()
    EndIf
    Int strikes = Utility.RandomInt(iMinMissileStrikes, iMaxMissileStrikes)
    If strikes < 1
        strikes = 1
    EndIf
    Int index = 0
    While index < strikes && IsRunning()
        Utility.Wait(Utility.RandomFloat(fMinOrbitalDelayTime, fMaxOrbitalDelayTime))
        If EN02_OrbitalStrikeShooter != None
            blast.PlaceAtMe(EN02_OrbitalStrikeShooter, 1, False, False, True)
        EndIf
        Utility.Wait(1.5)
        If OrbitalStrikeExplosion != None
            blast.PlaceAtMe(OrbitalStrikeExplosion, 1, False, False, True)
        EndIf
        index += 1
    EndWhile
    Utility.Wait(1.5)
    If IsRunning() && !IsStageDone(iWipeOutRemainingEnemiesStage)
        SetStage(iWipeOutRemainingEnemiesStage)
    EndIf
EndFunction

; Stages 15 and 17. The cargobot's hold package ends with these stages; it
; climbs away from the site and is removed once the flight finishes.
Function ENs02_ReleaseVertibot()
    If B21VertibotReleased
        Return
    EndIf
    B21VertibotReleased = True
    ObjectReference cargobot = ENs02_VertibotRef()
    If cargobot == None
        Return
    EndIf
    Float dirX = 0.0
    Float dirY = 1.0
    ObjectReference center = None
    If CenterMarkerActual != None
        center = CenterMarkerActual.GetReference()
    EndIf
    If center != None
        dirX = cargobot.GetPositionX() - center.GetPositionX()
        dirY = cargobot.GetPositionY() - center.GetPositionY()
        Float flyLength = Math.sqrt(dirX * dirX + dirY * dirY)
        If flyLength > 1.0
            dirX = dirX / flyLength
            dirY = dirY / flyLength
        Else
            dirX = 0.0
            dirY = 1.0
        EndIf
    EndIf
    RegisterForRemoteEvent(cargobot, "OnTranslationComplete")
    cargobot.TranslateTo(cargobot.GetPositionX() + dirX * fVBFlyOffDistance, cargobot.GetPositionY() + dirY * fVBFlyOffDistance, cargobot.GetPositionZ() + 1500.0, cargobot.GetAngleX(), cargobot.GetAngleY(), cargobot.GetAngleZ(), 700.0)
    StartTimer(30.0, 20002)
EndFunction

Function ENs02_DismissVertibot()
    CancelTimer(20002)
    ObjectReference cargobot = ENs02_VertibotRef()
    If cargobot == None
        Return
    EndIf
    UnregisterForRemoteEvent(cargobot, "OnTranslationComplete")
    cargobot.StopTranslation()
    cargobot.DisableNoWait()
    cargobot.Delete()
    If Vertibot != None
        Vertibot.Clear()
    EndIf
EndFunction

Function ENs02_BeginWrapUp()
    CancelTimer(iHordeTimerOffset)
    CancelTimer(iOrientationTimerOffset)
    CancelTimer(iFleeTimerID)
    CancelTimer(20000)
    CancelTimer(20003)
    StartTimer(60.0, 20001)
EndFunction

Function ENs02_Shutdown()
    CancelTimer(iHordeTimerOffset)
    CancelTimer(iOrientationTimerOffset)
    CancelTimer(iFleeTimerID)
    CancelTimer(20000)
    CancelTimer(20001)
    CancelTimer(20003)
    ENs02_DismissVertibot()
    ObjectReference syncMarker = None
    If SyncSoundMarker != None
        syncMarker = SyncSoundMarker.GetReference()
    EndIf
    If syncMarker != None
        syncMarker.DisableNoWait()
    EndIf
    Int index = 0
    While DishStatusCollections != None && index < DishStatusCollections.Length
        ENs02_DishStatusScript status = DishStatusCollections[index] as ENs02_DishStatusScript
        If status != None
            status.ENs02_ClearAllStatuses()
        EndIf
        index += 1
    EndWhile
    index = 0
    While Lures != None && index < Lures.GetCount()
        ENs02_SetLureState(Lures.GetAt(index), 0)
        index += 1
    EndWhile
    If LuresToOrient != None
        LuresToOrient.RemoveAll()
    EndIf
    If PlayerOrientedLure != None
        PlayerOrientedLure.RemoveAll()
    EndIf
    If LuresObj != None
        LuresObj.RemoveAll()
    EndIf
    B21:EnclaveEventSupport.ReleaseUnloadedActors(B21SpawnedEnemies)
    B21SpawnedEnemies = None
EndFunction

; Lure Array Component states are bound as 0 OFF, 1 searching, 2 ON.
Function ENs02_SetLureState(ObjectReference akLure, Int aiStateIndex)
    DefaultMultiStateActivator dish = akLure as DefaultMultiStateActivator
    If dish != None && dish.CurrentStateIndex != aiStateIndex
        dish.SetLocalState(aiStateIndex)
    EndIf
EndFunction

ObjectReference Function ENs02_VertibotRef()
    If Vertibot == None
        Return None
    EndIf
    Return Vertibot.GetReference()
EndFunction

ObjectReference Function ENs02_BlastMarkerRef()
    If BlastMarker == None
        Return None
    EndIf
    Return BlastMarker.GetReference()
EndFunction
