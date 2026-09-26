; Dropped Connection controller. FO76 ran the quest timers, the orbital drop
; and the regional encounter wave on the server. The FO4 substitute keeps the
; retained stage constants and timer IDs: 100 event expiry (source
; QuestExpireGlobal_Event, ignored once stage 100 is done), 102 the final loot
; window (ENz01_FinalQuestTimerLength), 5 the drop-progress poll. Local ID 103
; is the wrap-up before Stop.

Event OnQuestInit()
    B21SpawnedEnemies = new Actor[0]
    iActivationsCompleted = 0
    iOrbitalDropFailSafeCount = 0
    bDropCompletionOnce = False
    bGiantAllowed = False
    bLargeAllowed = False
    If ENz01_ActiveQuestDistance != None
        fJoinDistance = ENz01_ActiveQuestDistance.GetValue()
        fRemoveDistance = fJoinDistance
    EndIf
    Location site = None
    If ParentLocation != None
        site = ParentLocation.GetLocation()
    EndIf
    If site != None
        bGiantAllowed = LocEncSupportGiant != None && site.HasKeyword(LocEncSupportGiant)
        bLargeAllowed = LocEncSupportLarge != None && site.HasKeyword(LocEncSupportLarge)
    EndIf
    Parent.OnQuestInit()
EndEvent

Event OnQuestShutdown()
    ENz01_Shutdown()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iInitialQuestTimerID
        If ENz01_EventActive() && !IsStageDone(iEnemiesDeadStage) && !IsStageDone(160)
            SetStage(160)
        EndIf
    ElseIf aiTimerID == iUnlockQuestTimerID
        If IsRunning() && !IsStageDone(iSuccessfulCompletionStage) && !IsStageDone(165)
            SetStage(165)
        EndIf
    ElseIf aiTimerID == iDropProgressID
        ENz01_PollDrop()
    ElseIf aiTimerID == 103
        If IsRunning()
            Stop()
        EndIf
    EndIf
EndEvent

Bool Function ENz01_EventActive()
    Return IsRunning() && !IsStopping() && !IsStageDone(iSuccessfulCompletionStage) && !IsStageDone(160) && !IsStageDone(165)
EndFunction

Function ENz01_BeginStartup()
    ObjectReference crate = ENz01_Crate()
    If crate != None
        crate.Lock(True)
    EndIf
    GlobalVariable expireLength = Game.GetFormFromFile(0x0038093D, "SeventySix.esm") as GlobalVariable
    Float expireSeconds = 0.0
    If expireLength != None
        expireSeconds = expireLength.GetValue()
    EndIf
    If expireSeconds > 0.0
        StartTimer(expireSeconds, iInitialQuestTimerID)
    EndIf
EndFunction

; Returns True once every triangulation point is oriented and the drop stage is set.
Bool Function ENz01_RecordOrientation()
    iActivationsCompleted = 0
    If IsStageDone(iTriggerOneCompletedStage)
        iActivationsCompleted += 1
    EndIf
    If IsStageDone(iTriggerTwoCompletedStage)
        iActivationsCompleted += 1
    EndIf
    If IsStageDone(iTriggerThreeCompletedStage)
        iActivationsCompleted += 1
    EndIf
    If iActivationsCompleted < iActivationsRequired
        Return False
    EndIf
    If ENz01_EventActive() && !IsStageDone(50)
        SetStage(50)
    EndIf
    Return True
EndFunction

Function ENz01_BeginOrbitalDrop()
    If !ENz01_EventActive()
        Return
    EndIf
    ObjectReference decal = None
    If DecalMarker != None
        decal = DecalMarker.GetReference()
    EndIf
    If decal != None
        If ActiveDecalMarker != None
            ActiveDecalMarker.ForceRefTo(decal)
        EndIf
        decal.EnableNoWait()
    EndIf
    ObjectReference crate = ENz01_Crate()
    If crate == None
        bDropCompletionOnce = True
        SetStage(iKillEnemiesStage)
        Return
    EndIf
    crate.Lock(True)
    crate.EnableNoWait()
    ENz01_ResourceDropRefScript dropCrate = crate as ENz01_ResourceDropRefScript
    If dropCrate != None
        dropCrate.ENz01_BeginDrop()
    EndIf
    iOrbitalDropFailSafeCount = 0
    bDropCompletionOnce = False
    StartTimer(iDropProgressLength, iDropProgressID)
EndFunction

Function ENz01_PollDrop()
    If !ENz01_EventActive() || bDropCompletionOnce
        Return
    EndIf
    Float progress = 1.0
    ENz01_ResourceDropRefScript dropCrate = ENz01_Crate() as ENz01_ResourceDropRefScript
    If dropCrate != None
        progress = dropCrate.ENz01_DropProgress()
    EndIf
    iOrbitalDropFailSafeCount += 1
    If progress >= 1.0 || iOrbitalDropFailSafeCount >= 80
        bDropCompletionOnce = True
        If !IsStageDone(iKillEnemiesStage)
            SetStage(iKillEnemiesStage)
        EndIf
    Else
        StartTimer(iDropProgressLength, iDropProgressID)
    EndIf
EndFunction

Function ENz01_BeginDefense()
    If !ENz01_EventActive()
        Return
    EndIf
    ENEvent_SayToPlayer(ENz01_HostilesDetected)
    DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
    RefCollectionAlias enemies = GetAlias(13) as RefCollectionAlias
    ReferenceAlias spawnAlias = GetAlias(26) as ReferenceAlias
    ObjectReference spawnCenter = None
    If spawnAlias != None
        spawnCenter = spawnAlias.GetReference()
    EndIf
    If spawnCenter == None && CenterMarkerActual != None
        spawnCenter = CenterMarkerActual.GetReference()
    EndIf
    Int species = ENz01_SiteSpecies()
    Int enemyIndex = 0
    While spawnCenter != None && enemies != None && enemyIndex < 4 && ENz01_EventActive()
        Actor enemy = B21:EnclaveEventSupport.PlaceOnRing(spawnCenter, B21:EnclaveEventSupport.SpeciesForm(species, enemyIndex == 0))
        If enemy != None
            enemies.AddRef(enemy)
            If B21SpawnedEnemies == None
                B21SpawnedEnemies = new Actor[0]
            EndIf
            B21SpawnedEnemies.Add(enemy)
        EndIf
        enemyIndex += 1
    EndWhile
    If waves != None
        waves.StartLocalEncounterWave(0)
    EndIf
EndFunction

; The bound EnemyWaveEntries allow ghouls, liberators, super mutants and scorched
; at every site; the mole miner and rare entries need regions or support
; keywords no Dropped Connection site carries. Watoga's own pool (robots) has no
; allowed entry, so it falls back to the wave's default ghouls.
Int Function ENz01_SiteSpecies()
    ObjectReference center = None
    If CenterMarkerActual != None
        center = CenterMarkerActual.GetReference()
    EndIf
    If B21:EnclaveEventSupport.CenterInLocation(center, 0x0010CCEE)
        Return B21:EnclaveEventSupport.PickOne(3, 7, 0, 1)
    ElseIf B21:EnclaveEventSupport.CenterInLocation(center, 0x0000414B)
        Return 1
    ElseIf B21:EnclaveEventSupport.CenterInLocation(center, 0x00070368)
        Return B21:EnclaveEventSupport.PickOne(1, 0)
    ElseIf B21:EnclaveEventSupport.CenterInLocation(center, 0x0006DE2D)
        Return 0
    EndIf
    Return 3
EndFunction

Function ENz01_UnlockDrop(Float afLootSeconds)
    CancelTimer(iInitialQuestTimerID)
    ObjectReference crate = ENz01_Crate()
    If crate != None
        crate.Lock(False)
        If ENz01_LL_QuestReward_OrbitalDrop != None
            crate.AddItem(ENz01_LL_QuestReward_OrbitalDrop, 1, True)
        EndIf
    EndIf
    If !IsStageDone(110)
        SetStage(110)
    EndIf
    If afLootSeconds > 0.0
        StartTimer(afLootSeconds, iUnlockQuestTimerID)
    EndIf
EndFunction

Function ENz01_CloseDrop()
    ObjectReference crate = ENz01_Crate()
    If crate != None
        crate.Lock(True)
    EndIf
EndFunction

Function ENz01_BeginWrapUp()
    CancelTimer(iInitialQuestTimerID)
    CancelTimer(iUnlockQuestTimerID)
    CancelTimer(iDropProgressID)
    StartTimer(60.0, 103)
EndFunction

Function ENz01_Shutdown()
    CancelTimer(iInitialQuestTimerID)
    CancelTimer(iClearEnemiesQuestTimerID)
    CancelTimer(iUnlockQuestTimerID)
    CancelTimer(iDropProgressID)
    CancelTimer(103)
    ObjectReference crate = ENz01_Crate()
    If crate != None
        ENz01_ResourceDropRefScript dropCrate = crate as ENz01_ResourceDropRefScript
        If dropCrate != None
            dropCrate.ENz01_EndDrop()
        EndIf
        crate.Lock(True)
        crate.DisableNoWait()
    EndIf
    ObjectReference decal = None
    If DecalMarker != None
        decal = DecalMarker.GetReference()
    EndIf
    If decal != None
        decal.DisableNoWait()
    EndIf
    If ActiveDecalMarker != None
        ActiveDecalMarker.Clear()
    EndIf
    B21:EnclaveEventSupport.ReleaseUnloadedActors(B21SpawnedEnemies)
    B21SpawnedEnemies = None
EndFunction

ObjectReference Function ENz01_Crate()
    If RewardChest == None
        Return None
    EndIf
    Return RewardChest.GetReference()
EndFunction
