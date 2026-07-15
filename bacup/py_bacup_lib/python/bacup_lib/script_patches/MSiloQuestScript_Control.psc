Event OnQuestInit()
    Initialize()
EndEvent

Function Initialize()
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
    Control_LaunchPrepRobotsAliveMax = LaunchControlRobotData.Length
    isEventEnabled = True
    UpdateTerminals()
EndFunction

MSiloPersonalQuestScript Function GetPersonalQuest()
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    If personalQuest != None && !personalQuest.IsRunning()
        personalQuest.Start()
    EndIf
    Return personalQuest as MSiloPersonalQuestScript
EndFunction

Function StartLaunchPrep()
    If Control_LaunchPrepPhase >= CONST_Control_LaunchPrepPhase1 && Control_LaunchPrepPhase < CONST_Control_LaunchPrepPhaseComplete
        Return
    EndIf
    Control_LaunchPrepPhase = CONST_Control_LaunchPrepPhase1
    Control_LaunchControlTerminalStatus = CONST_Control_LaunchControlTerminalStatusLaunchPrepActive
    Control_LaunchPrepPointsCurrent = 0.0
    Control_LaunchPrepPercent = 0.0
    GetPersonalQuest().TryToSetStage(520)
    If MSiloPersonal_Control_02_LaunchPrepStart != None
        MSiloPersonal_Control_02_LaunchPrepStart.Start()
    EndIf
    SetLightingState(CONST_Control_LightingStateLaunchReady)
    UpdateTerminals()
    StartTimer(CONST_Control_LaunchPrepTimerDelay, CONST_Control_LaunchPrepTimerID)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != CONST_Control_LaunchPrepTimerID || Control_LaunchPrepPhase < CONST_Control_LaunchPrepPhase1 || Control_LaunchPrepPhase >= CONST_Control_LaunchPrepPhaseComplete
        Return
    EndIf

    Float increment = CONST_Control_LaunchPrepIncrementPerSecond_FirstRobot * CONST_Control_LaunchPrepTimerDelay
    If Control_LaunchPrepRobotsAlive > 1
        increment += ((Control_LaunchPrepRobotsAlive - 1) as Float) * CONST_Control_LaunchPrepIncrementPerSecond_EachAdditionalRobot * CONST_Control_LaunchPrepTimerDelay
    EndIf
    Control_LaunchPrepPointsCurrent += increment
    Control_LaunchPrepPercent = (Control_LaunchPrepPointsCurrent / CONST_Control_LaunchPrepPointsMax) * 100.0

    If Control_LaunchPrepPercent >= 75.0 && Control_LaunchPrepPhase < CONST_Control_LaunchPrepPhase3
        Control_LaunchPrepPhase = CONST_Control_LaunchPrepPhase3
        MSiloPersonal_Control_06_LaunchPrep075.Start()
    ElseIf Control_LaunchPrepPercent >= 50.0 && Control_LaunchPrepPhase < CONST_Control_LaunchPrepPhase2
        Control_LaunchPrepPhase = CONST_Control_LaunchPrepPhase2
        MSiloPersonal_Control_05_LaunchPrep050.Start()
    ElseIf Control_LaunchPrepPercent >= 25.0 && Control_LaunchPrepPhase == CONST_Control_LaunchPrepPhase1
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
    MSiloPersonal_Control_07_LaunchPrepComplete.Start()
    SetLightingState(CONST_Control_LightingStateNormal)
    UpdateTerminals()
EndFunction

Function ReplaceLaunchChief(Int aiIndex)
    If aiIndex < 0 || aiIndex >= LaunchControlRobotData.Length
        Return
    EndIf
    LaunchControlRobotDatum robotData = LaunchControlRobotData[aiIndex]
    If robotData.RobotRef != None && !robotData.RobotRef.IsDead()
        Return
    EndIf

    ObjectReference spawnMarker = robotData.RobotFabricator.GetReference()
    If spawnMarker == None
        spawnMarker = robotData.RobotStationMarker.GetReference()
    EndIf
    If spawnMarker == None || robotData.RobotActorBase == None
        Return
    EndIf

    Actor spawnedRobot = spawnMarker.PlaceAtMe(robotData.RobotActorBase) as Actor
    If spawnedRobot != None
        ObjectReference stationMarker = robotData.RobotStationMarker.GetReference()
        If stationMarker != None
            spawnedRobot.MoveTo(stationMarker)
        EndIf
        robotData.RobotRef = spawnedRobot
        robotData.RobotIsAlive = True
        robotData.RobotIsActive = True
        Control_LaunchPrepRobotsAlive += 1
        UpdateTerminals()
    EndIf
EndFunction

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
EndFunction
