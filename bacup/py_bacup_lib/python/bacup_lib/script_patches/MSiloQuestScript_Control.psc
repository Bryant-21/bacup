Event OnQuestInit()
    Initialize()
EndEvent

Function Initialize()
    If isEventEnabled
        Return
    EndIf
    isEventEnabled = True
    Quest siloQuest = Self as Quest
    MSiloControl = Self
    MSiloMain = siloQuest as MSiloQuestScript_Main
    MSiloReactor = siloQuest as MSiloQuestScript_Reactor
    MSiloStorage = siloQuest as MSiloQuestScript_Storage
    MSiloOperations = siloQuest as MSiloQuestScript_Operations
    MSiloResidential = siloQuest as MSiloQuestScript_Residential
    CONST_Control_EntryStage = 500
    CONST_Control_InitiateLaunchPrep = 510
    CONST_Control_CompleteLaunchPrep = 520
    CONST_Control_CompletedLaunchPrep = 530
    CONST_Control_GiveMidquestReward = 598
    Control_LaunchPrepPhase = CONST_Control_LaunchPrepPhaseNotStarted
    Control_LaunchControlTerminalStatus = CONST_Control_LaunchControlTerminalStatusLaunchPrepInactive
    Control_LaunchPrepPercent = 0.0
    Control_LaunchPrepPointsCurrent = 0.0
    Control_LaunchPrepRobotsAliveMax = 0
    spawningLaunchPrepRobot = False
    Int i = 0
    While i < LaunchControlRobotData.Length
        LaunchControlRobotData[i].RobotIsActive = False
        LaunchControlRobotData[i].RobotIsAlive = False
        LaunchControlRobotData[i].RobotRef = None
        i += 1
    EndWhile
    ReconcileLaunchChiefs()
    UpdateTerminals()
EndFunction

MSiloPersonalQuestScript Function GetPersonalQuest()
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    MSiloPersonalQuestScript personal = personalQuest as MSiloPersonalQuestScript
    If personal != None && !personalQuest.IsRunning()
        personal.EnsureSiloStarted(Game.GetPlayer().GetCurrentLocation())
    EndIf
    Return personal
EndFunction

Function StartLaunchPrep()
    If !IsRunning() || Control_LaunchPrepPhase >= CONST_Control_LaunchPrepPhaseComplete
        Return
    EndIf
    If Control_LaunchPrepPhase >= CONST_Control_LaunchPrepPhase1
        ReconcileLaunchChiefs()
        UpdateTerminals()
        StartTimer(CONST_Control_LaunchPrepTimerDelay, CONST_Control_LaunchPrepTimerID)
        Return
    EndIf
    Control_LaunchPrepPhase = CONST_Control_LaunchPrepPhase1
    Control_LaunchControlTerminalStatus = CONST_Control_LaunchControlTerminalStatusLaunchPrepActive
    Control_LaunchPrepPointsCurrent = 0.0
    Control_LaunchPrepPercent = 0.0
    GetPersonalQuest().TryToSetStage(520)
    ActivateLaunchChiefs(1)
    If MSiloPersonal_Control_02_LaunchPrepStart != None
        MSiloPersonal_Control_02_LaunchPrepStart.Start()
    EndIf
    SetLightingState(CONST_Control_LightingStateLaunchReady)
    UpdateTerminals()
    StartTimer(CONST_Control_LaunchPrepTimerDelay, CONST_Control_LaunchPrepTimerID)
EndFunction

Event OnTimer(Int aiTimerID)
    If !IsRunning() || aiTimerID != CONST_Control_LaunchPrepTimerID || Control_LaunchPrepPhase < CONST_Control_LaunchPrepPhase1 || Control_LaunchPrepPhase >= CONST_Control_LaunchPrepPhaseComplete
        Return
    EndIf

    ReconcileLaunchChiefs()
    Float previousPercent = Control_LaunchPrepPercent
    If Control_LaunchPrepRobotsAlive > 0
        Float increment = CONST_Control_LaunchPrepIncrementPerSecond_FirstRobot * CONST_Control_LaunchPrepTimerDelay
        increment += ((Control_LaunchPrepRobotsAlive - 1) as Float) * CONST_Control_LaunchPrepIncrementPerSecond_EachAdditionalRobot * CONST_Control_LaunchPrepTimerDelay
        Control_LaunchPrepPointsCurrent += increment
    EndIf
    Control_LaunchPrepPercent = (Control_LaunchPrepPointsCurrent / CONST_Control_LaunchPrepPointsMax) * 100.0

    If Control_LaunchPrepPointsCurrent >= CONST_Control_LaunchPrepPhase3PointThreshold && Control_LaunchPrepPhase < CONST_Control_LaunchPrepPhase3
        Control_LaunchPrepPhase = CONST_Control_LaunchPrepPhase3
        ActivateLaunchChiefs(LaunchControlRobotData.Length)
    ElseIf Control_LaunchPrepPointsCurrent >= CONST_Control_LaunchPrepPhase2PointThreshold && Control_LaunchPrepPhase < CONST_Control_LaunchPrepPhase2
        Control_LaunchPrepPhase = CONST_Control_LaunchPrepPhase2
        ActivateLaunchChiefs(3)
    EndIf
    If previousPercent < 75.0 && Control_LaunchPrepPercent >= 75.0 && MSiloPersonal_Control_06_LaunchPrep075 != None
        MSiloPersonal_Control_06_LaunchPrep075.Start()
    ElseIf previousPercent < 50.0 && Control_LaunchPrepPercent >= 50.0 && MSiloPersonal_Control_05_LaunchPrep050 != None
        MSiloPersonal_Control_05_LaunchPrep050.Start()
    ElseIf previousPercent < 25.0 && Control_LaunchPrepPercent >= 25.0 && MSiloPersonal_Control_04_LaunchPrep025 != None
        MSiloPersonal_Control_04_LaunchPrep025.Start()
    EndIf

    If Control_LaunchPrepPointsCurrent >= CONST_Control_LaunchPrepPointsMax
        Control_LaunchPrepPercent = 100.0
        GetPersonalQuest().TryToSetStage(530)
    Else
        UpdateTerminals()
        StartTimer(CONST_Control_LaunchPrepTimerDelay, CONST_Control_LaunchPrepTimerID)
    EndIf
EndEvent

Function CompleteLaunchPrep()
    If Control_LaunchPrepPhase >= CONST_Control_LaunchPrepPhaseComplete
        Return
    EndIf
    Control_LaunchPrepPhase = CONST_Control_LaunchPrepPhaseComplete
    CancelTimer(CONST_Control_LaunchPrepTimerID)
    Control_LaunchControlTerminalStatus = CONST_Control_LaunchControlTerminalStatusLaunchPrepCompleted
    Control_LaunchPrepPercent = 100.0
    ObjectReference targetingComputer = MSilo_Control_TargetingComputer.GetReference()
    Quest nukeMasterQuest = Game.GetFormFromFile(0x002D0F67, "SeventySix.esm") as Quest
    If nukeMasterQuest != None && !nukeMasterQuest.IsRunning()
        nukeMasterQuest.Start()
    EndIf
    If targetingComputer != None
        targetingComputer.BlockActivation(False, False)
        targetingComputer.SetActivateTextOverride(None)
    EndIf
    If MSiloPersonal_Control_07_LaunchPrepComplete != None
        MSiloPersonal_Control_07_LaunchPrepComplete.Start()
    EndIf
    Int i = MSilo_Control_EnemyRefCollection.GetCount() - 1
    While i >= 0
        Actor defender = MSilo_Control_EnemyRefCollection.GetAt(i) as Actor
        If defender != None && !defender.IsDead()
            defender.Kill()
        EndIf
        i -= 1
    EndWhile
    SetLightingState(CONST_Control_LightingStateNormal)
    UpdateTerminals()
EndFunction

Function ReplaceLaunchChief(Int aiIndex)
    If !IsRunning() || spawningLaunchPrepRobot || Control_LaunchPrepPhase < CONST_Control_LaunchPrepPhase1 || Control_LaunchPrepPhase >= CONST_Control_LaunchPrepPhaseComplete || aiIndex < 0 || aiIndex >= LaunchControlRobotData.Length
        Return
    EndIf
    LaunchControlRobotDatum robotData = LaunchControlRobotData[aiIndex]
    If !robotData.RobotIsActive || (robotData.RobotRef != None && !robotData.RobotRef.IsDead())
        Return
    EndIf

    ObjectReference spawnMarker = robotData.RobotFabricator.GetReference()
    If spawnMarker == None
        spawnMarker = robotData.RobotStationMarker.GetReference()
    EndIf
    If spawnMarker == None || robotData.RobotActorBase == None
        Return
    EndIf

    spawningLaunchPrepRobot = True
    UpdateTerminals()
    Actor spawnedRobot = spawnMarker.PlaceActorAtMe(robotData.RobotActorBase, robotData.RobotLevelMod)
    If spawnedRobot != None
        ObjectReference stationMarker = robotData.RobotStationMarker.GetReference()
        If stationMarker != None
            spawnedRobot.SetLinkedRef(stationMarker)
            spawnedRobot.MoveTo(stationMarker)
        EndIf
        robotData.RobotRef = spawnedRobot
        robotData.RobotIsAlive = True
        robotData.RobotIsActive = True
        LaunchControlRobotData[aiIndex] = robotData
        MSilo_Control_CrewChiefs.AddRef(spawnedRobot)
        RegisterForRemoteEvent(spawnedRobot, "OnDeath")
        spawnedRobot.EvaluatePackage()
        hasSpawnedALaunchPrepRobot = True
        If robotData.SceneToPlayWhenActivated != None
            robotData.SceneToPlayWhenActivated.Start()
        EndIf
    EndIf
    spawningLaunchPrepRobot = False
    ReconcileLaunchChiefs()
    UpdateTerminals()
EndFunction

Function ActivateLaunchChiefs(Int aiCount)
    Int i = 0
    While i < aiCount && i < LaunchControlRobotData.Length
        If !LaunchControlRobotData[i].RobotIsActive
            LaunchControlRobotData[i].RobotIsActive = True
            ReplaceLaunchChief(i)
        EndIf
        i += 1
    EndWhile
    ReconcileLaunchChiefs()
EndFunction

Function ReconcileLaunchChiefs()
    Int previousAlive = Control_LaunchPrepRobotsAlive
    Control_LaunchPrepRobotsAlive = 0
    Control_LaunchPrepRobotsAliveMax = 0
    Control_LaunchPrepRobotsInPosition = 0
    Int i = 0
    While i < LaunchControlRobotData.Length
        LaunchControlRobotDatum robotData = LaunchControlRobotData[i]
        Bool alive = robotData.RobotRef != None && !robotData.RobotRef.IsDead()
        If robotData.RobotIsActive
            Control_LaunchPrepRobotsAliveMax += 1
            If alive
                Control_LaunchPrepRobotsAlive += 1
                ObjectReference stationMarker = robotData.RobotStationMarker.GetReference()
                If stationMarker != None && robotData.RobotRef.GetDistance(stationMarker) <= 128.0
                    Control_LaunchPrepRobotsInPosition += 1
                EndIf
            ElseIf robotData.RobotIsAlive && robotData.SceneToPlayWhenDestroyed != None
                robotData.SceneToPlayWhenDestroyed.Start()
            EndIf
        EndIf
        robotData.RobotIsAlive = alive
        LaunchControlRobotData[i] = robotData
        i += 1
    EndWhile
    Control_LaunchPrepRobotsNeedingRestart = Control_LaunchPrepRobotsAliveMax - Control_LaunchPrepRobotsAlive
    If Control_LaunchPrepPhase >= CONST_Control_LaunchPrepPhase1 && Control_LaunchPrepPhase < CONST_Control_LaunchPrepPhaseComplete && Control_LaunchPrepRobotsAlive < previousAlive
        If Control_LaunchPrepRobotsAlive == 0 && MSiloPersonal_Control_25_AllChiefsDestroyed != None
            MSiloPersonal_Control_25_AllChiefsDestroyed.Start()
        ElseIf MSiloPersonal_Control_26_ProgressSlowed != None
            MSiloPersonal_Control_26_ProgressSlowed.Start()
        EndIf
    EndIf
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    UnregisterForRemoteEvent(akSender, "OnDeath")
    ReconcileLaunchChiefs()
    UpdateTerminals()
EndEvent

Event OnQuestShutdown()
    CancelTimer(CONST_Control_LaunchPrepTimerID)
    UnregisterForAllRemoteEvents()
    isEventEnabled = False
    spawningLaunchPrepRobot = False
EndEvent

Function SetLightingState(Int aiState)
    If Control_LightingState == aiState
        Return
    EndIf
    MSilo_Control_LightsOffEnableMarker.GetReference().Disable()
    MSilo_Control_LightsOnEnableMarker.GetReference().Disable()
    MSilo_Control_LightsLaunchEnableMarker.GetReference().Disable()
    If aiState == CONST_Control_LightingStateOff
        MSilo_Control_LightsOffEnableMarker.GetReference().Enable()
    ElseIf aiState == CONST_Control_LightingStateLaunchReady
        MSilo_Control_LightsLaunchEnableMarker.GetReference().Enable()
    Else
        MSilo_Control_LightsOnEnableMarker.GetReference().Enable()
    EndIf
    Control_LightingState = aiState
EndFunction

Function UpdateTerminal(ObjectReference akTerminalRef)
    If akTerminalRef != None
        akTerminalRef.SetValue(MSilo_Control_LaunchControlTerminalStatus, Control_LaunchControlTerminalStatus as Float)
        akTerminalRef.SetValue(MSilo_Control_LaunchPrepPercent, Control_LaunchPrepPercent)
    EndIf
EndFunction

Function UpdateTerminals()
    Int i = 0
    While i < MSilo_Control_LaunchControlTerminals.GetCount()
        UpdateTerminal(MSilo_Control_LaunchControlTerminals.GetAt(i))
        i += 1
    EndWhile
    i = 0
    While i < LaunchControlRobotData.Length
        LaunchControlRobotDatum robotData = LaunchControlRobotData[i]
        ObjectReference fabricatorTerminal = robotData.RobotFabricatorTerminal.GetReference()
        Int terminalStatus = CONST_Control_RobotFabricatorTerminalStatusBusy
        robotData.RobotState = MSilo_Control_RobotFabricatorStatusInactive
        If robotData.RobotIsAlive
            terminalStatus = CONST_Control_RobotFabricatorTerminalStatusActive
            robotData.RobotState = MSilo_Control_RobotFabricatorStatusActive
        ElseIf robotData.RobotIsActive && Control_LaunchPrepPhase < CONST_Control_LaunchPrepPhaseComplete
            If spawningLaunchPrepRobot
                robotData.RobotState = MSilo_Control_RobotFabricatorStatusBusy
            Else
                terminalStatus = CONST_Control_RobotFabricatorTerminalStatusReadyForRespawn
                robotData.RobotState = MSilo_Control_RobotFabricatorStatusDestroyed
            EndIf
        EndIf
        LaunchControlRobotData[i] = robotData
        If fabricatorTerminal != None
            fabricatorTerminal.SetValue(MSilo_Control_RobotFabricatorTerminalIndex, i as Float)
            fabricatorTerminal.SetValue(MSilo_Control_RobotFabricatorTerminalStatus, terminalStatus as Float)
            UpdateTerminal(fabricatorTerminal)
        EndIf
        i += 1
    EndWhile
EndFunction
