Actor Function ResolveLocalPlayer()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && Alias_Player != None && Alias_Player.GetReference() != playerRef
        Alias_Player.ForceRefTo(playerRef)
    EndIf
    Return playerRef
EndFunction

ObjectReference Function ResolveAliasReference(ReferenceAlias targetAlias)
    If targetAlias != None
        Return targetAlias.GetReference()
    EndIf
    Return None
EndFunction

Function SetAliasEnabled(ReferenceAlias targetAlias, Bool shouldEnable)
    ObjectReference targetRef = ResolveAliasReference(targetAlias)
    If targetRef != None
        If shouldEnable
            targetRef.Enable()
        Else
            targetRef.Disable()
        EndIf
    EndIf
EndFunction

Function SetCollectionEnabled(RefCollectionAlias targetCollection, Bool shouldEnable)
    Int index = 0
    While targetCollection != None && index < targetCollection.GetCount()
        ObjectReference targetRef = targetCollection.GetAt(index)
        If targetRef != None
            If shouldEnable
                targetRef.Enable()
            Else
                targetRef.Disable()
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function MoveActorAliasTo(ReferenceAlias actorAlias, ReferenceAlias markerAlias)
    Actor actorRef = None
    ObjectReference markerRef = ResolveAliasReference(markerAlias)
    If actorAlias != None
        actorRef = actorAlias.GetActorReference()
    EndIf
    If actorRef != None && markerRef != None
        actorRef.Enable()
        actorRef.MoveTo(markerRef)
        actorRef.EvaluatePackage()
    EndIf
EndFunction

Function StartSceneIfIdle(Scene sceneToStart)
    If sceneToStart != None && !sceneToStart.IsPlaying()
        sceneToStart.Start()
    EndIf
EndFunction

Function AddConversation(Scene sceneToAdd)
    If sceneToAdd != None
        AvailableConversations.Add(sceneToAdd)
    EndIf
EndFunction

Function BuildAvailableConversations()
    AvailableConversations = new Scene[0]
    ConversationSceneSeed = 0
    Actor playerRef = ResolveLocalPlayer()
    If playerRef != None
        If playerRef.GetValue(BS01_MQ03_FieldTesting_RecruitedColinPutnam_AV) > 0.0 || playerRef.GetValue(BS01_MQ03_FieldTesting_RecruitedMartyPutnam_AV) > 0.0
            AddConversation(BS01_MQ08_Defense_BonusConvFieldTesting_Scene)
        EndIf
        If playerRef.GetValue(BS01_MQ04_Arms_WeaponChoice) == 2.0
            AddConversation(BS01_MQ08_Defense_BonusConvBiAGaveWeapons_Scene)
        EndIf
        If playerRef.GetValue(BS01_MQ05_Raiders_AV_Negotiation) == 2.0
            AddConversation(BS01_MQ08_Defense_BonusConvPRStrongArmedRaider_Scene)
        ElseIf playerRef.GetValue(BS01_MQ05_Raiders_AV_Negotiation) == 4.0
            AddConversation(BS01_MQ08_Defense_BonusConvPRBackedDown_Scene)
        EndIf
        If playerRef.GetValue(BS01_MQ06_Settlers_Decision) == 2.0
            AddConversation(BS01_MQ08_Defense_BonusConvSDFoundationKeptWeapons_Scene)
        ElseIf playerRef.GetValue(BS01_MQ06_Settlers_Decision) == 1.0
            AddConversation(BS01_MQ08_Defense_BonusConvSDGuardFoundation_Scene)
        EndIf
    EndIf
    AddConversation(BS01_MQ08_Defense_BonusConvGeneric01_Scene)
    AddConversation(BS01_MQ08_Defense_BonusConvGeneric02_Scene)
EndFunction

Function RecordConversationChoice(Scene chosenScene)
    Int chosenStage = 0
    If chosenScene == BS01_MQ08_Defense_BonusConvBiAGaveWeapons_Scene
        chosenStage = BonusConvBiAGaveWeaponsStage
    ElseIf chosenScene == BS01_MQ08_Defense_BonusConvFieldTesting_Scene
        chosenStage = BonusConvFieldTestingStage
    ElseIf chosenScene == BS01_MQ08_Defense_BonusConvPRBackedDown_Scene
        chosenStage = BonusConvPRBackedDownStage
    ElseIf chosenScene == BS01_MQ08_Defense_BonusConvPRStrongArmedRaider_Scene
        chosenStage = BonusConvPRStrongArmedRaiderStage
    ElseIf chosenScene == BS01_MQ08_Defense_BonusConvSDFoundationKeptWeapons_Scene
        chosenStage = BonusConvSDFoundationKeptWeaponsStage
    ElseIf chosenScene == BS01_MQ08_Defense_BonusConvSDGuardFoundation_Scene
        chosenStage = BonusConvSDGuardFoundationStage
    EndIf
    If chosenStage > 0 && !IsStageDone(chosenStage)
        SetStage(chosenStage)
    EndIf
EndFunction

Function PlayNextConversation()
    If AvailableConversations == None || AvailableConversations.Length == 0
        BuildAvailableConversations()
    EndIf
    If AvailableConversations.Length > 0
        Int conversationIndex = ConversationSceneSeed % AvailableConversations.Length
        Scene chosenScene = AvailableConversations[conversationIndex]
        ConversationSceneSeed += 1
        RecordConversationChoice(chosenScene)
        StartSceneIfIdle(chosenScene)
    EndIf
EndFunction

Function SetBombPlaced(ReferenceAlias bombAlias)
    ObjectReference bombRef = ResolveAliasReference(bombAlias)
    Actor playerRef = ResolveLocalPlayer()
    If bombRef != None
        bombRef.Enable()
        If PlaceBombSound != None
            PlaceBombSound.Play(bombRef)
        EndIf
    EndIf
    If playerRef != None && playerRef.GetItemCount(BS01_MQ08_Defense_Bomb_MiscItem) > 0
        playerRef.RemoveItem(BS01_MQ08_Defense_Bomb_MiscItem, 1, True)
    EndIf
    If IsStageDone(510) && IsStageDone(520) && IsStageDone(530) && IsStageDone(540) && !IsStageDone(AllBombsPlantedStage)
        SetStage(AllBombsPlantedStage)
    EndIf
EndFunction

Function SyncBombStatics()
    Bool showBombs = !IsStageDone(PostBombsStage)
    SetAliasEnabled(Alias_Static_BombA, showBombs && IsStageDone(510))
    SetAliasEnabled(Alias_Static_BombB, showBombs && IsStageDone(520))
    SetAliasEnabled(Alias_Static_BombC, showBombs && IsStageDone(530))
    SetAliasEnabled(Alias_Static_BombD, showBombs && IsStageDone(540))
    If IsStageDone(510) && IsStageDone(520) && IsStageDone(530) && IsStageDone(540) && !IsStageDone(AllBombsPlantedStage)
        SetStage(AllBombsPlantedStage)
    EndIf
EndFunction

Function StartBrotherhoodBarricadeTravel()
    StartSceneIfIdle(BS01_MQ08_Defense_BarricadeTravel_Scene)
    CancelTimer(BrotherhoodNPCSpawnTimerID)
    StartTimer(NPCEntranceTime, BrotherhoodNPCSpawnTimerID)
EndFunction

Function FinishBrotherhoodBarricadeTravel()
    MoveActorAliasTo(Alias_Actor_Valdez_Dungeon, Alias_Marker_Valdez_BarricadeXMarker)
    MoveActorAliasTo(Alias_Actor_Shin_Dungeon, Alias_Marker_Shin_BarricadeXMarker)
    MoveActorAliasTo(Alias_Actor_Rahmani_Dungeon, Alias_Marker_Rahmani_BarricadeXMarker)
    If !IsStageDone(DungeonNPCsAtBarricadeStage)
        SetStage(DungeonNPCsAtBarricadeStage)
    EndIf
EndFunction

Function StartBombDetonationTimer()
    CancelTimer(BombDetonationTimerID)
    StartTimer(BombDetonationTime, BombDetonationTimerID)
EndFunction

Function DetonateBombs()
    If IsStageDone(PostBombsStage)
        Return
    EndIf
    Int index = 0
    While Alias_Markers_ExplosionMarkers != None && index < Alias_Markers_ExplosionMarkers.GetCount()
        ObjectReference explosionMarker = Alias_Markers_ExplosionMarkers.GetAt(index)
        If explosionMarker != None && BombExplosion != None
            explosionMarker.PlaceAtMe(BombExplosion)
        EndIf
        index += 1
    EndWhile
    SetCollectionEnabled(Alias_MovableStatics_VentCollapses, True)
    SetCollectionEnabled(Alias_Statics_Bombs, False)
    SetAliasEnabled(Alias_Marker_SpawnHolePlugEnableMarker, True)
    SetAliasEnabled(Alias_Marker_KillTriggerEnableMarker, True)
    index = 0
    While AmbientEnemyEnableMarkers != None && index < AmbientEnemyEnableMarkers.Length
        SetAliasEnabled(AmbientEnemyEnableMarkers[index], False)
        index += 1
    EndWhile
    Actor playerRef = ResolveLocalPlayer()
    If playerRef != None && CameraShakeSpell != None
        CameraShakeSpell.Cast(playerRef, playerRef)
    EndIf
    If !IsStageDone(PostBombsStage)
        SetStage(PostBombsStage)
    EndIf
EndFunction

Bool Function IsLocalDefenseActor(Actor enemyRef, ActorBase rangedBase, ActorBase meleeBase, ActorBase houndBase)
    If enemyRef == None || !enemyRef.IsCreated()
        Return False
    EndIf
    ActorBase enemyBase = enemyRef.GetActorBase()
    Return enemyBase == rangedBase || enemyBase == meleeBase || enemyBase == houndBase
EndFunction

Function CleanupLocalDefenseWave(RefCollectionAlias localEnemies, ActorBase rangedBase, ActorBase meleeBase, ActorBase houndBase)
    If localEnemies == None || rangedBase == None || meleeBase == None || houndBase == None
        Return
    EndIf
    Int enemyIndex = localEnemies.GetCount() - 1
    While enemyIndex >= 0
        Actor enemyRef = localEnemies.GetAt(enemyIndex) as Actor
        If IsLocalDefenseActor(enemyRef, rangedBase, meleeBase, houndBase)
            UnregisterForRemoteEvent(enemyRef, "OnDeath")
            localEnemies.RemoveRef(enemyRef)
            enemyRef.Disable()
            enemyRef.Delete()
        EndIf
        enemyIndex -= 1
    EndWhile
EndFunction

Function CleanupAllLocalDefenseWaves()
    ActorBase rangedBase = Game.GetFormFromFile(0x00075335, "Fallout4.esm") as ActorBase
    ActorBase meleeBase = Game.GetFormFromFile(0x00088F14, "Fallout4.esm") as ActorBase
    ActorBase houndBase = Game.GetFormFromFile(0x000948B3, "Fallout4.esm") as ActorBase
    CleanupLocalDefenseWave(GetAlias(43) as RefCollectionAlias, rangedBase, meleeBase, houndBase)
    CleanupLocalDefenseWave(GetAlias(47) as RefCollectionAlias, rangedBase, meleeBase, houndBase)
    CleanupLocalDefenseWave(GetAlias(48) as RefCollectionAlias, rangedBase, meleeBase, houndBase)
    CleanupLocalDefenseWave(GetAlias(49) as RefCollectionAlias, rangedBase, meleeBase, houndBase)
EndFunction

Bool Function ReconcileLocalDefenseWave(Int spawnAliasID, Int collectionAliasID)
    ReferenceAlias spawnAlias = GetAlias(spawnAliasID) as ReferenceAlias
    RefCollectionAlias localEnemies = GetAlias(collectionAliasID) as RefCollectionAlias
    ObjectReference spawnCenter = ResolveAliasReference(spawnAlias)
    Actor playerRef = ResolveLocalPlayer()
    ActorBase rangedBase = Game.GetFormFromFile(0x00075335, "Fallout4.esm") as ActorBase
    ActorBase meleeBase = Game.GetFormFromFile(0x00088F14, "Fallout4.esm") as ActorBase
    ActorBase houndBase = Game.GetFormFromFile(0x000948B3, "Fallout4.esm") as ActorBase
    Int rangedCount = 0
    Int meleeCount = 0
    Int houndCount = 0
    Int enemyIndex = 0

    If spawnCenter == None || localEnemies == None || playerRef == None || rangedBase == None || meleeBase == None || houndBase == None
        Return False
    EndIf
    While enemyIndex < localEnemies.GetCount()
        Actor existingEnemyRef = localEnemies.GetAt(enemyIndex) as Actor
        If IsLocalDefenseActor(existingEnemyRef, rangedBase, meleeBase, houndBase)
            ActorBase enemyBase = existingEnemyRef.GetActorBase()
            If enemyBase == rangedBase
                rangedCount += 1
            ElseIf enemyBase == meleeBase
                meleeCount += 1
            ElseIf enemyBase == houndBase
                houndCount += 1
            EndIf
        EndIf
        enemyIndex += 1
    EndWhile
    If rangedCount == 1 && meleeCount == 1 && houndCount == 1
        enemyIndex = 0
        While enemyIndex < localEnemies.GetCount()
            Actor reconciledEnemyRef = localEnemies.GetAt(enemyIndex) as Actor
            If IsLocalDefenseActor(reconciledEnemyRef, rangedBase, meleeBase, houndBase) && !reconciledEnemyRef.IsDead()
                UnregisterForRemoteEvent(reconciledEnemyRef, "OnDeath")
                RegisterForRemoteEvent(reconciledEnemyRef, "OnDeath")
                reconciledEnemyRef.StartCombat(playerRef, True)
            EndIf
            enemyIndex += 1
        EndWhile
        Return True
    EndIf
    CleanupLocalDefenseWave(localEnemies, rangedBase, meleeBase, houndBase)
    enemyIndex = 0
    While enemyIndex < 3
        ActorBase enemyBaseToSpawn = rangedBase
        If enemyIndex == 1
            enemyBaseToSpawn = meleeBase
        ElseIf enemyIndex == 2
            enemyBaseToSpawn = houndBase
        EndIf
        Bool spawnCommitted = False
        Actor spawnedEnemyRef = spawnCenter.PlaceActorAtMe(enemyBaseToSpawn, 0)
        If spawnedEnemyRef != None
            localEnemies.AddRef(spawnedEnemyRef)
            If localEnemies.Find(spawnedEnemyRef) >= 0
                spawnCommitted = True
            Else
                spawnedEnemyRef.Disable()
                spawnedEnemyRef.Delete()
            EndIf
        EndIf
        If !spawnCommitted
            CleanupLocalDefenseWave(localEnemies, rangedBase, meleeBase, houndBase)
            Return False
        EndIf
        enemyIndex += 1
    EndWhile
    enemyIndex = 0
    While enemyIndex < localEnemies.GetCount()
        Actor committedEnemyRef = localEnemies.GetAt(enemyIndex) as Actor
        If IsLocalDefenseActor(committedEnemyRef, rangedBase, meleeBase, houndBase)
            RegisterForRemoteEvent(committedEnemyRef, "OnDeath")
            committedEnemyRef.StartCombat(playerRef, True)
        EndIf
        enemyIndex += 1
    EndWhile
    Return True
EndFunction

Function StartAllLocalDefenseWaves()
    If !IsStageDone(500) || IsStageDone(PostBombsStage)
        Return
    EndIf
    CancelTimer(30)
    Bool waveAReady = ReconcileLocalDefenseWave(42, 43)
    Bool waveBReady = ReconcileLocalDefenseWave(44, 47)
    Bool waveCReady = ReconcileLocalDefenseWave(45, 48)
    Bool waveDReady = ReconcileLocalDefenseWave(46, 49)
    If !waveAReady || !waveBReady || !waveCReady || !waveDReady
        CleanupAllLocalDefenseWaves()
        StartTimer(5.0, 30)
    EndIf
EndFunction

Function CompleteLocalDefenseWaveIfAllDead(RefCollectionAlias localEnemies)
    ActorBase rangedBase = Game.GetFormFromFile(0x00075335, "Fallout4.esm") as ActorBase
    ActorBase meleeBase = Game.GetFormFromFile(0x00088F14, "Fallout4.esm") as ActorBase
    ActorBase houndBase = Game.GetFormFromFile(0x000948B3, "Fallout4.esm") as ActorBase
    Int rangedCount = 0
    Int meleeCount = 0
    Int houndCount = 0
    Int enemyIndex = 0

    If localEnemies == None || rangedBase == None || meleeBase == None || houndBase == None || localEnemies.GetCount() <= 0
        Return
    EndIf
    While enemyIndex < localEnemies.GetCount()
        Actor scannedEnemyRef = localEnemies.GetAt(enemyIndex) as Actor
        If IsLocalDefenseActor(scannedEnemyRef, rangedBase, meleeBase, houndBase)
            ActorBase enemyBase = scannedEnemyRef.GetActorBase()
            If enemyBase == rangedBase
                rangedCount += 1
            ElseIf enemyBase == meleeBase
                meleeCount += 1
            ElseIf enemyBase == houndBase
                houndCount += 1
            EndIf
            If !scannedEnemyRef.IsDead()
                Return
            EndIf
        EndIf
        enemyIndex += 1
    EndWhile
    If rangedCount != 1 || meleeCount != 1 || houndCount != 1
        Return
    EndIf
    enemyIndex = 0
    While enemyIndex < localEnemies.GetCount()
        Actor completedEnemyRef = localEnemies.GetAt(enemyIndex) as Actor
        If IsLocalDefenseActor(completedEnemyRef, rangedBase, meleeBase, houndBase)
            UnregisterForRemoteEvent(completedEnemyRef, "OnDeath")
        EndIf
        enemyIndex += 1
    EndWhile
EndFunction

Function ReconcileRuntime()
    ResolveLocalPlayer()
    SyncBombStatics()
    If IsStageDone(250) && !IsStageDone(DungeonNPCsAtBarricadeStage)
        StartBrotherhoodBarricadeTravel()
    EndIf
    If IsStageDone(700) && !IsStageDone(PostBombsStage)
        StartBombDetonationTimer()
    EndIf
    If IsStageDone(500) && !IsStageDone(PostBombsStage)
        StartAllLocalDefenseWaves()
    EndIf
EndFunction

Event OnQuestInit()
    Actor playerRef = ResolveLocalPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    BuildAvailableConversations()
    SyncBombStatics()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 3 || auiStageID == 100
        BuildAvailableConversations()
    ElseIf auiStageID == 250
        StartBrotherhoodBarricadeTravel()
    ElseIf auiStageID == DungeonNPCsAtBarricadeStage
        CancelTimer(BrotherhoodNPCSpawnTimerID)
    ElseIf auiStageID == 450 || auiStageID == 475
        PlayNextConversation()
    ElseIf auiStageID == 510
        SetBombPlaced(Alias_Static_BombA)
    ElseIf auiStageID == 520
        SetBombPlaced(Alias_Static_BombB)
    ElseIf auiStageID == 530
        SetBombPlaced(Alias_Static_BombC)
    ElseIf auiStageID == 540
        SetBombPlaced(Alias_Static_BombD)
    ElseIf auiStageID == 500
        StartAllLocalDefenseWaves()
    ElseIf auiStageID == 700
        StartBombDetonationTimer()
    ElseIf auiStageID == PostBombsStage
        CancelTimer(BombDetonationTimerID)
        CancelTimer(30)
        CleanupAllLocalDefenseWaves()
        SyncBombStatics()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == BrotherhoodNPCSpawnTimerID && IsStageDone(250) && !IsStageDone(DungeonNPCsAtBarricadeStage)
        FinishBrotherhoodBarricadeTravel()
    ElseIf aiTimerID == BombDetonationTimerID && IsStageDone(700) && !IsStageDone(PostBombsStage)
        DetonateBombs()
    ElseIf aiTimerID == 30 && IsStageDone(500) && !IsStageDone(PostBombsStage)
        StartAllLocalDefenseWaves()
    EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    RefCollectionAlias localEnemies = GetAlias(43) as RefCollectionAlias
    If localEnemies != None && localEnemies.Find(akSender) >= 0
        CompleteLocalDefenseWaveIfAllDead(localEnemies)
        Return
    EndIf
    localEnemies = GetAlias(47) as RefCollectionAlias
    If localEnemies != None && localEnemies.Find(akSender) >= 0
        CompleteLocalDefenseWaveIfAllDead(localEnemies)
        Return
    EndIf
    localEnemies = GetAlias(48) as RefCollectionAlias
    If localEnemies != None && localEnemies.Find(akSender) >= 0
        CompleteLocalDefenseWaveIfAllDead(localEnemies)
        Return
    EndIf
    localEnemies = GetAlias(49) as RefCollectionAlias
    If localEnemies != None && localEnemies.Find(akSender) >= 0
        CompleteLocalDefenseWaveIfAllDead(localEnemies)
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        ReconcileRuntime()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(BrotherhoodNPCSpawnTimerID)
    CancelTimer(BombDetonationTimerID)
    CancelTimer(30)
    CleanupAllLocalDefenseWaves()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndEvent
