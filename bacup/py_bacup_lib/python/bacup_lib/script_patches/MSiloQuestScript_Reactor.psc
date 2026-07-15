Event OnQuestInit()
    Initialize()
EndEvent

Function Initialize()
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

Function ShowRepairInstructions()
    GetPersonalQuest().TryToSetStage(220)
EndFunction

Function ShutdownReactor()
    Reactor_ReactorStatus = CONST_Reactor_ReactorStatusRepairInProgress
    reactorStatusBroken = True
    GetPersonalQuest().TryToSetStage(230)
    UpdateTerminals()
EndFunction

Function RestartReactor()
    Reactor_ReactorStatus = CONST_Reactor_ReactorStatusRepairComplete
    Reactor_ReactorSecurityStatus = CONST_Reactor_ReactorSecurityStatusOverride
    reactorStatusBroken = False
    securityStatusNormal = True
    Reactor_IsRepaired = True
    GetPersonalQuest().TryToSetStage(240)
    GetPersonalQuest().TryToSetStage(250)
    OpenSecurityDoors()
    UpdateTerminals()
EndFunction

Function OverrideSecurityLockdown()
    Reactor_ReactorSecurityStatus = CONST_Reactor_ReactorSecurityStatusOverride
    securityStatusNormal = True
    GetPersonalQuest().TryToSetStage(210)
    OpenSecurityDoors()
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
    While i < MSilo_Reactor_ReactorRadiation.GetCount()
        MSilo_Reactor_ReactorRadiation.GetAt(i).Disable()
        i += 1
    EndWhile
    i = 0
    While i < MSilo_Reactor_EntryDoorFX.GetCount()
        MSilo_Reactor_EntryDoorFX.GetAt(i).Disable()
        i += 1
    EndWhile
    Reactor_SecurityDoorsOpen = True
EndFunction

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
