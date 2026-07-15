Event OnQuestInit()
    Initialize()
EndEvent

Function Initialize()
    Quest siloQuest = Self as Quest
    MSiloOperations = Self
    MSiloMain = siloQuest as MSiloQuestScript_Main
    MSiloControl = siloQuest as MSiloQuestScript_Control
    MSiloStorage = siloQuest as MSiloQuestScript_Storage
    MSiloReactor = siloQuest as MSiloQuestScript_Reactor
    MSiloResidential = siloQuest as MSiloQuestScript_Residential
    CONST_Operations_EntryStage = 300
    CONST_Operations_DestroyTheMainframe = 310
    CONST_Operations_CompletedEvent = 320
    CONST_Operations_AwardMidquestReward = 398
    Operations_MainframePanelsInitialCount = MSilo_Operations_MainframePanels.GetCount() as Float
    Operations_SecurityLevelCurrent = CONST_Operations_SecurityLevelMax
    Operations_SecurityLevelPercent = 100.0
    If Operations_MainframePanelsInitialCount > 0.0
        Operations_SecurityPointsPerMainframePanel = (CONST_Operations_SecurityLevelMax - CONST_Operations_SecurityLevelMin) / Operations_MainframePanelsInitialCount
    EndIf
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

Function HandlePanelDestroyed(ObjectReference akPanel)
    Int total = MSilo_Operations_MainframePanels.GetCount()
    Int destroyed = 0
    Int i = 0
    While i < total
        If MSilo_Operations_MainframePanels.GetAt(i).GetCurrentDestructionStage() > 0
            destroyed += 1
        EndIf
        i += 1
    EndWhile

    If total <= 0
        total = 1
        destroyed = 1
    EndIf
    Operations_SecurityLevelPercent = 100.0 - ((destroyed as Float) * 100.0 / (total as Float))
    Operations_SecurityLevelCurrent = CONST_Operations_SecurityLevelMin + ((CONST_Operations_SecurityLevelMax - CONST_Operations_SecurityLevelMin) * Operations_SecurityLevelPercent / 100.0)
    UpdateTerminals()
    If destroyed >= total
        SetLaserGridsOpen(True)
        GetPersonalQuest().TryToSetStage(320)
    EndIf
EndFunction

Function SetLaserGridsOpen(Bool abOpen)
    Int i = 0
    While i < MSilo_Operations_LaserGrids.GetCount()
        MSiloLaserGridScript grid = MSilo_Operations_LaserGrids.GetAt(i) as MSiloLaserGridScript
        If grid != None
            grid.SetOpen(abOpen)
        EndIf
        i += 1
    EndWhile
EndFunction

Function UpdateTerminal(ObjectReference akTerminalRef)
    If akTerminalRef != None
        akTerminalRef.SetValue(MSilo_Operations_SecurityLevelTerminalValue, Operations_SecurityLevelCurrent)
    EndIf
EndFunction

Function UpdateTerminals()
    Int i = 0
    While i < MSilo_Operations_MainframeTerminals.GetCount()
        UpdateTerminal(MSilo_Operations_MainframeTerminals.GetAt(i))
        i += 1
    EndWhile
EndFunction
