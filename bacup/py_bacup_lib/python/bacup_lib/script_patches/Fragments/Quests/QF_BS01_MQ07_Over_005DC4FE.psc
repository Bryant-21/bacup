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

Actor Function ResolveAliasActor(ReferenceAlias targetAlias)
    If targetAlias != None
        Return targetAlias.GetActorReference()
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

Function SetAliasActivationBlocked(ReferenceAlias targetAlias, Bool shouldBlock)
    ObjectReference targetRef = ResolveAliasReference(targetAlias)
    If targetRef != None
        targetRef.BlockActivation(shouldBlock, False)
    EndIf
EndFunction

Function MoveActorAliasTo(ReferenceAlias actorAlias, ReferenceAlias markerAlias)
    Actor actorRef = ResolveAliasActor(actorAlias)
    ObjectReference markerRef = ResolveAliasReference(markerAlias)
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

Function SaySODUSTopic(Topic topicToSay)
    Actor playerRef = ResolveLocalPlayer()
    If playerRef != None && topicToSay != None
        playerRef.Say(topicToSay, None, True)
    EndIf
EndFunction

Function EnsureAliasContainerItem(ReferenceAlias containerAlias, Form itemToAdd, ReferenceAlias itemAlias)
    ObjectReference containerRef = ResolveAliasReference(containerAlias)
    ObjectReference itemRef = ResolveAliasReference(itemAlias)
    If containerRef != None && itemToAdd != None && itemRef == None && containerRef.GetItemCount(itemToAdd) < 1
        containerRef.AddItem(itemToAdd, 1, True)
    EndIf
EndFunction

Function SetLocalPlayerValue(ActorValue valueToSet, Float value)
    Actor playerRef = ResolveLocalPlayer()
    If playerRef != None && valueToSet != None
        playerRef.SetValue(valueToSet, value)
    EndIf
EndFunction

RefCollectionAlias Function ResolveCafeteriaEnemies()
    GenericEWSModuleQuest encounterQuest = (Self as Quest) as GenericEWSModuleQuest
    If encounterQuest != None && encounterQuest.EWS_Enemies_RefCollection != None
        Return encounterQuest.EWS_Enemies_RefCollection
    EndIf
    Return GetAlias(64) as RefCollectionAlias
EndFunction

ObjectReference Function ResolveCafeteriaSpawnCenter()
    ObjectReference spawnCenter = ResolveAliasReference(Alias_SpawnCenter_Cafeteria)
    If spawnCenter == None
        spawnCenter = Game.GetFormFromFile(0x005DCA76, "SeventySix.esm") as ObjectReference
        If spawnCenter != None && Alias_SpawnCenter_Cafeteria != None
            Alias_SpawnCenter_Cafeteria.ForceRefTo(spawnCenter)
        EndIf
    EndIf
    Return spawnCenter
EndFunction

Bool Function HasCafeteriaEnemyReferences(RefCollectionAlias enemies)
    If enemies == None
        Return False
    EndIf

    Int enemyIndex = 0
    While enemyIndex < enemies.GetCount()
        Actor enemyRef = enemies.GetAt(enemyIndex) as Actor
        If enemyRef != None
            Return True
        EndIf
        enemyIndex += 1
    EndWhile
    Return False
EndFunction

Function SpawnCafeteriaEnemy(ObjectReference spawnCenter, RefCollectionAlias enemies, ActorBase enemyBase)
    If spawnCenter == None || enemies == None || enemyBase == None
        Return
    EndIf

    Actor enemyRef = spawnCenter.PlaceAtMe(enemyBase, 1, True, False, False) as Actor
    If enemyRef != None
        enemies.AddRef(enemyRef)
    EndIf
EndFunction

Function PopulateCafeteriaEncounter(ObjectReference spawnCenter, RefCollectionAlias enemies)
    ActorBase meleeScientist = Game.GetFormFromFile(0x005E52EC, "SeventySix.esm") as ActorBase
    ActorBase rifleScientist = Game.GetFormFromFile(0x005E52EB, "SeventySix.esm") as ActorBase
    ActorBase officer = Game.GetFormFromFile(0x005DCFB9, "SeventySix.esm") as ActorBase

    SpawnCafeteriaEnemy(spawnCenter, enemies, meleeScientist)
    SpawnCafeteriaEnemy(spawnCenter, enemies, rifleScientist)
    SpawnCafeteriaEnemy(spawnCenter, enemies, officer)
EndFunction

Function StartBoundActorCombat(ReferenceAlias actorAlias)
    Actor actorRef = ResolveAliasActor(actorAlias)
    Actor playerRef = ResolveLocalPlayer()
    If actorRef != None && playerRef != None && !actorRef.IsDead()
        actorRef.Enable()
        RegisterForRemoteEvent(actorRef, "OnDeath")
        actorRef.StartCombat(playerRef)
        actorRef.EvaluatePackage()
    EndIf
EndFunction

Function CleanupCafeteriaEncounter(RefCollectionAlias enemies)
    If enemies == None
        Return
    EndIf

    Int enemyIndex = enemies.GetCount() - 1
    While enemyIndex >= 0
        ObjectReference enemyRef = enemies.GetAt(enemyIndex)
        If enemyRef != None
            Actor enemyActor = enemyRef as Actor
            If enemyActor != None
                UnregisterForRemoteEvent(enemyActor, "OnDeath")
            EndIf
            enemies.RemoveRef(enemyRef)
            enemyRef.Disable(False)
            enemyRef.Delete()
        EndIf
        enemyIndex -= 1
    EndWhile
EndFunction

Function CompleteCafeteriaEncounterIfCleared()
    If !IsStageDone(2450) || IsStageDone(2500)
        Return
    EndIf

    RefCollectionAlias enemies = ResolveCafeteriaEnemies()
    If enemies == None || enemies.GetCount() == 0
        Return
    EndIf

    Bool observedEnemy = False
    Int enemyIndex = 0
    While enemyIndex < enemies.GetCount()
        Actor enemyRef = enemies.GetAt(enemyIndex) as Actor
        If enemyRef != None
            observedEnemy = True
            If !enemyRef.IsDead()
                Return
            EndIf
        EndIf
        enemyIndex += 1
    EndWhile
    If observedEnemy
        SetStage(2500)
        CleanupCafeteriaEncounter(enemies)
    EndIf
EndFunction

Function StartCafeteriaEncounter()
    RefCollectionAlias enemies = ResolveCafeteriaEnemies()
    If IsStageDone(2500)
        CleanupCafeteriaEncounter(enemies)
        Return
    EndIf

    If enemies == None
        Return
    EndIf

    ObjectReference spawnCenter = ResolveCafeteriaSpawnCenter()
    If !HasCafeteriaEnemyReferences(enemies)
        PopulateCafeteriaEncounter(spawnCenter, enemies)
    EndIf

    If !HasCafeteriaEnemyReferences(enemies)
        Return
    EndIf

    If spawnCenter != None
        spawnCenter.Enable()
    EndIf

    Actor playerRef = ResolveLocalPlayer()
    Int enemyIndex = 0
    While enemyIndex < enemies.GetCount()
        Actor enemyRef = enemies.GetAt(enemyIndex) as Actor
        If enemyRef != None && !enemyRef.IsDead()
            enemyRef.Enable()
            RegisterForRemoteEvent(enemyRef, "OnDeath")
            If playerRef != None
                enemyRef.StartCombat(playerRef)
            EndIf
            enemyRef.EvaluatePackage()
        EndIf
        enemyIndex += 1
    EndWhile
    CompleteCafeteriaEncounterIfCleared()
EndFunction

Function DestroyBoundTransmitter()
    Quests:BS01_MQ07_Over:TransmitterAnimScript transmitterScript = ResolveAliasReference(Alias_Activator_Transmitter) as Quests:BS01_MQ07_Over:TransmitterAnimScript
    If transmitterScript != None && transmitterScript.GetState() != "broken"
        transmitterScript.DestroyTransmitter()
        transmitterScript.GoToState("broken")
    EndIf
EndFunction

Function ReconcileBestDefensePlayerAlias(Actor playerRef)
    If playerRef == None || BS01_MQ08_Defense == None
        Return
    EndIf

    ReferenceAlias nextPlayerAlias = BS01_MQ08_Defense.GetAlias(0) as ReferenceAlias
    If nextPlayerAlias != None && nextPlayerAlias.GetReference() != playerRef
        nextPlayerAlias.ForceRefTo(playerRef)
    EndIf
EndFunction

Function SendBestDefenseStoryEvent()
    Actor playerRef = ResolveLocalPlayer()
    If playerRef == None || BS01_MQ08_Defense == None || BS01_MQ08_Defense_QuestStartKeyword == None
        Return
    EndIf

    If BS01_MQ08_Defense.IsCompleted()
        CancelTimer(0)
        Return
    EndIf

    If BS01_MQ08_Defense.IsRunning()
        ReconcileBestDefensePlayerAlias(playerRef)
        CancelTimer(0)
        Return
    EndIf

    Bool questStarted = BS01_MQ08_Defense_QuestStartKeyword.SendStoryEventAndWait(LocMountainsObservatoryIntLocation, playerRef)
    If questStarted || BS01_MQ08_Defense.IsRunning()
        ReconcileBestDefensePlayerAlias(playerRef)
        CancelTimer(0)
    ElseIf BS01_MQ08_Defense.IsCompleted()
        CancelTimer(0)
    Else
        StartTimer(5.0, 0)
    EndIf
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    Actor sentryRef = ResolveAliasActor(Alias_Actor_SentryBot)
    If akSender == sentryRef && IsStageDone(3000) && !IsStageDone(3100)
        SetStage(3100)
    Else
        CompleteCafeteriaEncounterIfCleared()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0 && IsStageDone(9000)
        SendBestDefenseStoryEvent()
    ElseIf aiTimerID == 0
        CancelTimer(0)
    EndIf
EndEvent

Function Fragment_Stage_0001_Item_00()
    If !IsStageDone(2830)
        SetStage(2830)
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    If !IsStageDone(3350)
        SetStage(3350)
    EndIf
EndFunction

Function Fragment_Stage_0008_Item_00()
    ResolveLocalPlayer()
    SetAliasEnabled(Alias_Actor_Valdez_FortAtlas, True)
    SetAliasEnabled(Alias_Furniture_ValdezEmptyTable, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
    ResolveLocalPlayer()
    SetAliasEnabled(Alias_ERF_EnableMarkerRef, True)
    SetLocalPlayerValue(BS01_RahmaniAwayValue, 1.0)
    SetLocalPlayerValue(BS01_ShinAwayValue, 1.0)
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = ResolveLocalPlayer()
    If playerRef != None && BS01_MQ01_Trust != None && BS01_MQ01_Trust.IsCompleted() && BS01_AV_IsInitiate != None && playerRef.GetValue(BS01_AV_IsInitiate) < 1.0
        playerRef.SetValue(BS01_AV_IsInitiate, 1.0)
    EndIf
    SetObjectiveDisplayed(5, True, True)
    If playerRef != None && LocMountainsObservatoryIntLocation != None && playerRef.IsInLocation(LocMountainsObservatoryIntLocation) && !IsStageDone(8)
        SetStage(8)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(10, True, True)
    StartSceneIfIdle(BS01_MQ07_Over_Intro)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True, True)
    SetAliasEnabled(Alias_Furniture_UltraciteBatteryTable, True)
EndFunction

Function Fragment_Stage_0290_Item_00()
    SetObjectiveDisplayed(20, True, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True, True)
    EnsureAliasContainerItem(Alias_Actor_RadioTowerCorpse, BS01_MQ07_Over_RadioTowerNote, Alias_QO_Book_RadioTowerNote)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(40, True, True)
EndFunction

Function Fragment_Stage_0430_Item_00()
    EnsureAliasContainerItem(Alias_Actor_RadioTowerCorpse, BS01_MQ07_Over_RadioTowerNote, Alias_QO_Book_RadioTowerNote)
EndFunction

Function Fragment_Stage_0440_Item_00()
    SetObjectiveDisplayed(45, True, True)
EndFunction

Function Fragment_Stage_0450_Item_00()
    Actor playerRef = ResolveLocalPlayer()
    If playerRef != None && BS01_MQ07_Over_UsedCircuitBreakerKeyword != None && !playerRef.HasKeyword(BS01_MQ07_Over_UsedCircuitBreakerKeyword)
        playerRef.AddKeyword(BS01_MQ07_Over_UsedCircuitBreakerKeyword)
    EndIf
    SetAliasEnabled(Alias_Activator_RadioTowerLaserGrid, False)
    SetObjectiveCompleted(45, True)
    SetObjectiveDisplayed(50, True, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(40, True)
    SetObjectiveCompleted(45, True)
    SetObjectiveDisplayed(50, True, True)
    SetAliasEnabled(Alias_Activator_ElevatorButton_Interior, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(60, True, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = ResolveLocalPlayer()
    If playerRef != None && EN02_JoinedEnclaveValue != None && playerRef.GetValue(EN02_JoinedEnclaveValue) > 0.0
        StartSceneIfIdle(BS01_MQ07_Over_SODUS_GreetEnclaveMember)
    Else
        StartSceneIfIdle(pSODUSIntroScene)
    EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
    SetObjectiveDisplayed(60, True, True)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SaySODUSTopic(pSODUSDecontamTopic)
EndFunction

Function Fragment_Stage_1150_Item_00()
    SaySODUSTopic(pSODUSDecontamSkipTopic)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SaySODUSTopic(pSODUSTask01)
EndFunction

Function Fragment_Stage_1300_Item_00()
    SaySODUSTopic(pSODUSBiohazard)
    SetObjectiveCompleted(60, True)
    SetObjectiveDisplayed(70, True, True)
EndFunction

Function Fragment_Stage_1350_Item_00()
    SaySODUSTopic(pSODUSBiohazardEscapeTopic)
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1360_Item_00()
    SaySODUSTopic(pSODUSBiohazardEscapeTopic)
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(70, True)
    SetObjectiveDisplayed(60, True, True)
EndFunction

Function Fragment_Stage_1500_Item_00()
    SaySODUSTopic(pSODUSTask02)
EndFunction

Function Fragment_Stage_1600_Item_00()
    SaySODUSTopic(pSODUSTask03)
EndFunction

Function Fragment_Stage_1700_Item_00()
    SaySODUSTopic(pSODUSHoldingCellOpenTopic)
EndFunction

Function Fragment_Stage_1800_Item_00()
    SaySODUSTopic(pSODUSTask04)
    EnsureAliasContainerItem(Alias_Actor_KeycardCorpse, BS01_MQ07_HoldingCellKeyCard01, Alias_QO_Keycard)
EndFunction

Function Fragment_Stage_1850_Item_00()
    EnsureAliasContainerItem(Alias_Actor_KeycardCorpse, BS01_MQ07_HoldingCellKeyCard01, Alias_QO_Keycard)
    SetObjectiveDisplayed(80, True, True)
EndFunction

Function Fragment_Stage_1860_Item_00()
    EnsureAliasContainerItem(Alias_Actor_KeycardCorpse, BS01_MQ07_HoldingCellKeyCard01, Alias_QO_Keycard)
    SetObjectiveDisplayed(80, True, True)
EndFunction

Function Fragment_Stage_1880_Item_00()
    SetObjectiveCompleted(80, True)
    SetObjectiveDisplayed(60, True, True)
EndFunction

Function Fragment_Stage_1900_Item_00()
    SaySODUSTopic(pSODUSWelcomeBack01Topic)
EndFunction

Function Fragment_Stage_2000_Item_00()
    SaySODUSTopic(pSODUSWelcomeBack02Topic)
EndFunction

Function Fragment_Stage_2050_Item_00()
    SaySODUSTopic(pSODUSTask05)
EndFunction

Function Fragment_Stage_2100_Item_00()
    SaySODUSTopic(pSODUSSupplyRoom01Topic)
EndFunction

Function Fragment_Stage_2200_Item_00()
    SaySODUSTopic(pSODUSSupplyRoom02Topic)
    StartBoundActorCombat(Alias_Actor_Mothman)
EndFunction

Function Fragment_Stage_2400_Item_00()
    SaySODUSTopic(pSODUSCafe01Topic)
    SetAliasActivationBlocked(pCafeDoorButton, True)
EndFunction

Function Fragment_Stage_2450_Item_00()
    SaySODUSTopic(pSODUSCafe02Topic)
    SetObjectiveCompleted(60, True)
    SetObjectiveDisplayed(85, True, True)
    StartCafeteriaEncounter()
EndFunction

Function Fragment_Stage_2500_Item_00()
    SetObjectiveCompleted(85, True)
    SetObjectiveDisplayed(60, True, True)
    SetAliasActivationBlocked(pCafeDoorButton, False)
EndFunction

Function Fragment_Stage_2550_Item_00()
    SetObjectiveDisplayed(60, True, True)
EndFunction

Function Fragment_Stage_2600_Item_00()
    SaySODUSTopic(pSODUSTask06)
EndFunction

Function Fragment_Stage_2700_Item_00()
    SaySODUSTopic(pSODUSTask07)
EndFunction

Function Fragment_Stage_2800_Item_00()
    SaySODUSTopic(pSODUSCommTopic)
    SetObjectiveCompleted(60, True)
    SetObjectiveDisplayed(90, True, True)
EndFunction

Function Fragment_Stage_2805_Item_00()
    SetObjectiveDisplayed(90, True, True)
EndFunction

Function Fragment_Stage_2810_Item_00()
    SaySODUSTopic(pSODUSCommTopic)
    SetObjectiveCompleted(90, True)
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_2820_Item_00()
    Actor playerRef = ResolveLocalPlayer()
    If playerRef != None && BS01_MQ07_Over_HandscanRegistrationKeyword != None && !playerRef.HasKeyword(BS01_MQ07_Over_HandscanRegistrationKeyword)
        playerRef.AddKeyword(BS01_MQ07_Over_HandscanRegistrationKeyword)
    EndIf
    SaySODUSTopic(pSODUSRegistrationTopic)
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_2825_Item_00()
    SetLocalPlayerValue(BS01_MQ07_Over_AV_MainframeAccess, 1.0)
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(110, True, True)
EndFunction

Function Fragment_Stage_2830_Item_00()
    SetObjectiveCompleted(110, True)
    SetObjectiveDisplayed(120, True, True)
    MoveActorAliasTo(Alias_Actor_Rahmani_EnclaveFacility, Alias_Marker_RahmaniLobby)
    MoveActorAliasTo(Alias_Actor_Shin_EnclaveFacility, Alias_Marker_ShinLobby)
    StartSceneIfIdle(BS01_MQ07_Over_BoSArriveAtFacility)
EndFunction

Function Fragment_Stage_2840_Item_00()
    SetObjectiveCompleted(120, True)
    SetObjectiveDisplayed(130, True, True)
EndFunction

Function Fragment_Stage_2850_Item_00()
    SetObjectiveDisplayed(130, True, True)
EndFunction

Function Fragment_Stage_2860_Item_00()
    SetObjectiveDisplayed(130, True, True)
EndFunction

Function Fragment_Stage_2890_Item_00()
    SetObjectiveCompleted(130, True)
    MoveActorAliasTo(Alias_Actor_Rahmani_EnclaveFacility, Alias_Marker_RahmaniMainframe)
    MoveActorAliasTo(Alias_Actor_Shin_EnclaveFacility, Alias_Marker_ShinMainframe)
EndFunction

Function Fragment_Stage_2900_Item_00()
    MoveActorAliasTo(Alias_Actor_Rahmani_EnclaveFacility, Alias_Marker_RahmaniMainframe)
    MoveActorAliasTo(Alias_Actor_Shin_EnclaveFacility, Alias_Marker_ShinMainframe)
EndFunction

Function Fragment_Stage_3000_Item_00()
    SetObjectiveCompleted(130, True)
    SetObjectiveDisplayed(140, True, True)
    SetAliasEnabled(Alias_Activator_SentryBotTrigger, True)
    StartSceneIfIdle(BS01_MQ07_Over_BoSReactToSODUS)
    Actor sentryRef = ResolveAliasActor(Alias_Actor_SentryBot)
    If sentryRef == None || sentryRef.IsDead()
        If !IsStageDone(3100)
            SetStage(3100)
        EndIf
    Else
        StartBoundActorCombat(Alias_Actor_SentryBot)
    EndIf
EndFunction

Function Fragment_Stage_3100_Item_00()
    SetObjectiveCompleted(140, True)
    SetObjectiveDisplayed(150, True, True)
    StartSceneIfIdle(BS01_MQ07_Over_BoSAfterCombat)
EndFunction

Function Fragment_Stage_3200_Item_00()
    SetObjectiveCompleted(150, True)
    SetObjectiveDisplayed(160, True, True)
    SetAliasEnabled(Alias_Furniture_UltraciteBatteryTable, True)
EndFunction

Function Fragment_Stage_3250_Item_00()
    Actor shinRef = ResolveAliasActor(Alias_Actor_Shin_EnclaveFacility)
    If shinRef != None
        shinRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_3300_Item_00()
    SetObjectiveCompleted(160, True)
    SetObjectiveDisplayed(170, True, True)
EndFunction

Function Fragment_Stage_3350_Item_00()
    SetObjectiveDisplayed(170, True, True)
EndFunction

Function Fragment_Stage_3400_Item_00()
    SetObjectiveDisplayed(170, True, True)
EndFunction

Function Fragment_Stage_4000_Item_00()
    SetObjectiveCompleted(170, True)
    StartSceneIfIdle(BS01_MQ07_Over_Sabotage)
EndFunction

Function Fragment_Stage_4100_Item_00()
    SetLocalPlayerValue(BS01_AV_LoyaltyChoice, 1.0)
    If !IsStageDone(4000)
        SetStage(4000)
    EndIf
EndFunction

Function Fragment_Stage_4200_Item_00()
    SetLocalPlayerValue(BS01_AV_LoyaltyChoice, 2.0)
    If !IsStageDone(4000)
        SetStage(4000)
    EndIf
EndFunction

Function Fragment_Stage_4300_Item_00()
    SetLocalPlayerValue(BS01_AV_LoyaltyChoice, 0.0)
    If !IsStageDone(4000)
        SetStage(4000)
    EndIf
EndFunction

Function Fragment_Stage_5000_Item_00()
    DestroyBoundTransmitter()
EndFunction

Function Fragment_Stage_5100_Item_00()
    SetAliasEnabled(Alias_SoundMarker_EmergencyAlert, True)
EndFunction

Function Fragment_Stage_5200_Item_00()
    SetAliasEnabled(Alias_SoundMarker_EmergencyAlert, False)
EndFunction

Function Fragment_Stage_9000_Item_00()
    ResolveLocalPlayer()
    CleanupCafeteriaEncounter(ResolveCafeteriaEnemies())
    SetAliasEnabled(Alias_SoundMarker_EmergencyAlert, False)
    SetLocalPlayerValue(BS01_RahmaniAwayValue, 0.0)
    SetLocalPlayerValue(BS01_ShinAwayValue, 0.0)
    SendBestDefenseStoryEvent()
EndFunction
