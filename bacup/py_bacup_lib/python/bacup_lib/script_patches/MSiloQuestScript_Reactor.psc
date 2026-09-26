Event OnQuestInit()
    Initialize()
EndEvent

Function Initialize()
    If isEventEnabled
        Return
    EndIf
    isEventEnabled = True
    Quest siloQuest = Self as Quest
    MSiloReactor = Self
    MSiloMain = siloQuest as MSiloQuestScript_Main
    MSiloControl = siloQuest as MSiloQuestScript_Control
    MSiloStorage = siloQuest as MSiloQuestScript_Storage
    MSiloOperations = siloQuest as MSiloQuestScript_Operations
    MSiloResidential = siloQuest as MSiloQuestScript_Residential
    CONST_Reactor_NoStageChange = 0
    CONST_Reactor_EntryStage = 200
    CONST_Reactor_EndTheSecurityLockdownStage = 210
    CONST_Reactor_ShutdownTheReactorStage = 220
    CONST_Reactor_StartRepairStage = 230
    CONST_Reactor_ReactorReadyForRestartStage = 240
    CONST_Reactor_EndRepairSuccessStage = 250
    CONST_Reactor_AwardMidquestReward = 298
    CONST_Reactor_ReactorStatusBroken = 0
    CONST_Reactor_ReactorStatusBusy = 1
    CONST_Reactor_ReactorStatusRepairInProgress = 2
    CONST_Reactor_ReactorStatusRepairReadyForRestart = 3
    CONST_Reactor_ReactorStatusRepairComplete = 4
    CONST_Reactor_ReactorSecurityStatusNormal = 0
    CONST_Reactor_ReactorSecurityStatusOverride = 1
    Reactor_ReactorStatus = CONST_Reactor_ReactorStatusBroken
    Reactor_ReactorSecurityStatus = CONST_Reactor_ReactorSecurityStatusNormal
    reactorStatusBroken = True
    Reactor_IsRepaired = False
    Reactor_SecurityDoorsOpen = False
    Int i = 0
    While i < MSilo_Reactor_ReactorDestructibles.GetCount()
        DefaultFixable2StateActivator pipe = MSilo_Reactor_ReactorDestructibles.GetAt(i) as DefaultFixable2StateActivator
        If pipe != None
            pipe.SetBlockActivationWhenBroken(True)
            pipe.SetActivatorBroken(True, True)
        EndIf
        i += 1
    EndWhile
    ReconcileRepairs()
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

Function ShowRepairInstructions()
    GetPersonalQuest().TryToSetStage(220)
EndFunction

Function ShutdownReactor()
    If !IsRunning() || Reactor_ReactorStatus != CONST_Reactor_ReactorStatusBroken || Reactor_IsRepaired
        Return
    EndIf
    Reactor_ReactorStatus = CONST_Reactor_ReactorStatusRepairInProgress
    reactorStatusBroken = True
    objectiveTimeRemaining = CONST_Reactor_RepairObjectiveTime
    SetPipeRepairAllowed(True)
    SetRadiationEnabled(False)
    GetPersonalQuest().TryToSetStage(230)
    GetPersonalQuest().SetObjectiveCompleted(230, False)
    GetPersonalQuest().SetObjectiveDisplayed(230)
    GetPersonalQuest().SetObjectiveDisplayed(231)
    If MSiloPersonal_Reactor_03_RepairEventShutdown != None
        MSiloPersonal_Reactor_03_RepairEventShutdown.Start()
    EndIf
    ReconcileRepairs()
    UpdateRepairTimer()
    UpdateTerminals()
    StartTimer(1.0, CONST_Reactor_RepairEventTimerID)
EndFunction

Function RestartReactor()
    If !IsRunning() || Reactor_IsRepaired || (Reactor_ReactorStatus != CONST_Reactor_ReactorStatusRepairInProgress && Reactor_ReactorStatus != CONST_Reactor_ReactorStatusRepairReadyForRestart)
        Return
    EndIf
    ReconcileRepairs()
    If Reactor_DestroyedDestructibles.Length > 0 || Reactor_IntactDestructibles.Length == 0
        If MSiloPersonal_Reactor_12_RepairEventRestartFail != None
            MSiloPersonal_Reactor_12_RepairEventRestartFail.Start()
        EndIf
        Return
    EndIf
    CancelTimer(CONST_Reactor_RepairEventTimerID)
    Reactor_ReactorStatus = CONST_Reactor_ReactorStatusRepairComplete
    Reactor_ReactorSecurityStatus = CONST_Reactor_ReactorSecurityStatusOverride
    reactorStatusBroken = False
    securityStatusNormal = True
    Reactor_IsRepaired = True
    SetPipeRepairAllowed(False)
    SetRadiationEnabled(False)
    GetPersonalQuest().TryToSetStage(240)
    GetPersonalQuest().TryToSetStage(250)
    OpenSecurityDoors()
    If MSiloPersonal_Reactor_13_RepairEventRestartSucceed != None
        MSiloPersonal_Reactor_13_RepairEventRestartSucceed.Start()
    EndIf
    UpdateTerminals()
EndFunction

Function OverrideSecurityLockdown()
    If !IsRunning() || Reactor_ReactorSecurityStatus == CONST_Reactor_ReactorSecurityStatusOverride
        Return
    EndIf
    Reactor_ReactorSecurityStatus = CONST_Reactor_ReactorSecurityStatusOverride
    securityStatusNormal = True
    GetPersonalQuest().TryToSetStage(210)
    OpenSecurityDoors()
    If MSiloPersonal_Reactor_15_OverrideLockdown != None
        MSiloPersonal_Reactor_15_OverrideLockdown.Start()
    EndIf
    UpdateTerminals()
EndFunction

Function OpenSecurityDoors()
    Int i = 0
    While i < MSilo_Reactor_ReactorSecurityDoors.GetCount()
        ObjectReference doorRef = MSilo_Reactor_ReactorSecurityDoors.GetAt(i)
        doorRef.Lock(False)
        doorRef.SetOpen(True)
        i += 1
    EndWhile
    i = 0
    While i < MSilo_Reactor_EntryDoorFX.GetCount()
        MSilo_Reactor_EntryDoorFX.GetAt(i).Disable()
        i += 1
    EndWhile
    Reactor_SecurityDoorsOpen = True
EndFunction

Function SetPipeRepairAllowed(Bool abAllowed)
    Int i = 0
    While i < MSilo_Reactor_ReactorDestructibles.GetCount()
        DefaultFixable2StateActivator pipe = MSilo_Reactor_ReactorDestructibles.GetAt(i) as DefaultFixable2StateActivator
        If pipe != None
            pipe.SetBlockActivationWhenBroken(!abAllowed)
        EndIf
        i += 1
    EndWhile
EndFunction

Function SetRadiationEnabled(Bool abEnabled)
    Int i = 0
    While i < MSilo_Reactor_ReactorRadiation.GetCount()
        ObjectReference radiationRef = MSilo_Reactor_ReactorRadiation.GetAt(i)
        If abEnabled
            radiationRef.Enable()
        Else
            radiationRef.Disable()
        EndIf
        i += 1
    EndWhile
EndFunction

Function ReconcileRepairs()
    Reactor_DestroyedDestructibles = new ObjectReference[0]
    Reactor_IntactDestructibles = new ObjectReference[0]
    Int i = 0
    While i < MSilo_Reactor_ReactorDestructibles.GetCount()
        ObjectReference pipeRef = MSilo_Reactor_ReactorDestructibles.GetAt(i)
        DefaultFixable2StateActivator pipe = pipeRef as DefaultFixable2StateActivator
        If pipe == None || pipe.IsActivatorBroken()
            Reactor_DestroyedDestructibles.Add(pipeRef)
        Else
            Reactor_IntactDestructibles.Add(pipeRef)
        EndIf
        i += 1
    EndWhile
    MSilo_Reactor_ReactorDestructibles_Count = i
    repairProgressPercent = 0.0
    If i > 0
        repairProgressPercent = (Reactor_IntactDestructibles.Length as Float) * 100.0 / (i as Float)
    EndIf
    Reactor_ShouldShowRemainingDestructibles = Reactor_DestroyedDestructibles.Length <= CONST_Reactor_RepairObjectiveShowRemainingDestructiblesThreshold
    MSiloPersonalQuestScript personal = GetPersonalQuest()
    If personal != None && personal.IsRunning()
        personal.Reactor_ShouldShowRemainingDestructibles = Reactor_ShouldShowRemainingDestructibles
        personal.Reactor_ReactorStatusBroken = reactorStatusBroken
        personal.Reactor_SecurityStatusNormal = securityStatusNormal
        personal.MSilo_Reactor_ReactorDestructiblesRemaining.RemoveAll()
        Int j = 0
        While j < Reactor_DestroyedDestructibles.Length
            personal.MSilo_Reactor_ReactorDestructiblesRemaining.AddRef(Reactor_DestroyedDestructibles[j])
            j += 1
        EndWhile
        If Reactor_ReactorStatus == CONST_Reactor_ReactorStatusRepairInProgress && i > 0 && Reactor_DestroyedDestructibles.Length == 0
            Reactor_ReactorStatus = CONST_Reactor_ReactorStatusRepairReadyForRestart
            personal.TryToSetStage(240)
            personal.SetObjectiveCompleted(230)
            personal.SetObjectiveCompleted(231)
            personal.SetObjectiveDisplayed(240)
        EndIf
    EndIf
EndFunction

Function UpdateRepairTimer()
    GlobalVariable timerGlobal = Game.GetFormFromFile(0x003DE7DC, "SeventySix.esm") as GlobalVariable
    If timerGlobal != None
        timerGlobal.SetValue(objectiveTimeRemaining as Float)
        MSiloPersonalQuestScript personal = GetPersonalQuest()
        If personal != None && personal.IsRunning()
            personal.UpdateCurrentInstanceGlobal(timerGlobal)
        EndIf
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If !IsRunning() || aiTimerID != CONST_Reactor_RepairEventTimerID || (Reactor_ReactorStatus != CONST_Reactor_ReactorStatusRepairInProgress && Reactor_ReactorStatus != CONST_Reactor_ReactorStatusRepairReadyForRestart)
        Return
    EndIf
    objectiveTimeRemaining -= 1
    ReconcileRepairs()
    UpdateRepairTimer()
    If objectiveTimeRemaining <= 0
        If Reactor_DestroyedDestructibles.Length == 0 && Reactor_IntactDestructibles.Length > 0
            RestartReactor()
        Else
            Reactor_ReactorStatus = CONST_Reactor_ReactorStatusBroken
            SetPipeRepairAllowed(False)
            SetRadiationEnabled(True)
            MSiloPersonalQuestScript personal = GetPersonalQuest()
            personal.SetObjectiveDisplayed(230, False)
            personal.SetObjectiveDisplayed(231, False)
            personal.SetObjectiveDisplayed(240, False)
            personal.SetObjectiveCompleted(220, False)
            personal.SetObjectiveDisplayed(220)
            If MSiloPersonal_Reactor_12_RepairEventRestartFail != None
                MSiloPersonal_Reactor_12_RepairEventRestartFail.Start()
            EndIf
        EndIf
    Else
        If objectiveTimeRemaining == 60 && MSiloPersonal_Reactor_04_RepairEvent1MinuteWarning != None
            MSiloPersonal_Reactor_04_RepairEvent1MinuteWarning.Start()
        EndIf
        StartTimer(1.0, CONST_Reactor_RepairEventTimerID)
    EndIf
    UpdateTerminals()
EndEvent

Event OnQuestShutdown()
    CancelTimer(CONST_Reactor_RepairEventTimerID)
    isEventEnabled = False
EndEvent

Function UpdateTerminal(ObjectReference akTerminalRef)
    If akTerminalRef != None
        akTerminalRef.SetValue(MSilo_Reactor_ReactorStatusValue, Reactor_ReactorStatus as Float)
        akTerminalRef.SetValue(MSilo_Reactor_ReactorSecurityStatusValue, Reactor_ReactorSecurityStatus as Float)
        akTerminalRef.SetValue(MSilo_Reactor_ReactorRepairPercentValue, repairProgressPercent)
    EndIf
EndFunction

Function UpdateTerminals()
    Int i = 0
    While i < MSilo_Reactor_ReactorControlTerminals.GetCount()
        UpdateTerminal(MSilo_Reactor_ReactorControlTerminals.GetAt(i))
        i += 1
    EndWhile
EndFunction
