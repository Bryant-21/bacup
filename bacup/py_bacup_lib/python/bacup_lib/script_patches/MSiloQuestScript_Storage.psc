Event OnQuestInit()
    Initialize()
EndEvent

Function Initialize()
    Quest siloQuest = Self as Quest
    MSiloStorage = Self
    MSiloMain = siloQuest as MSiloQuestScript_Main
    MSiloControl = siloQuest as MSiloQuestScript_Control
    MSiloReactor = siloQuest as MSiloQuestScript_Reactor
    MSiloOperations = siloQuest as MSiloQuestScript_Operations
    MSiloResidential = siloQuest as MSiloQuestScript_Residential
    CONST_Storage_EntryStage = 400
    CONST_Storage_ReadFacilitiesMainframeTerminal = 419
    CONST_Storage_ReplaceTheMainframeCores = 420
    CONST_Storage_OpenTheSecurityDoor = 430
    CONST_Storage_CompletedEvent = 440
    CONST_Storage_GiveMidquestReward = 498
    Storage_MainframeCoresMax = MSilo_Storage_MainframePanelsAll.GetCount()
    Storage_MainframeCoresCurrent = MSilo_Storage_MainframePanels_Replaced.GetCount()
    If Storage_MainframeCoresMax <= 0
        Storage_MainframeCoresMax = 1
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

Function HandlePanelActivation(ObjectReference akPanel, ObjectReference akActivator)
    If akActivator != Game.GetPlayer() || akPanel.GetValue(MSilo_Storage_FacilitiesMainframeRepairedValue) >= 1.0
        Return
    EndIf
    If akActivator.GetItemCount(MSilo_Storage_ReplacementPanelIntact) < 1
        MSilo_Storage_FacilitiesMainframeSlotNeedsCore.Show()
        Return
    EndIf

    akActivator.RemoveItem(MSilo_Storage_ReplacementPanelIntact, 1, True)
    akPanel.SetValue(MSilo_Storage_FacilitiesMainframeRepairedValue, 1.0)
    MSilo_Storage_MainframePanels_Replaced.AddRef(akPanel)
    Storage_MainframeCoresCurrent += 1
    UpdateTerminals()
    If Storage_MainframeCoresCurrent >= Storage_MainframeCoresMax
        GetPersonalQuest().TryToSetStage(430)
    EndIf
EndFunction

Function FinishMainframeBoot(ObjectReference akTerminalRef)
    If akTerminalRef != None
        akTerminalRef.SetValue(MSilo_Storage_SuccessfulBootValue, 1.0)
    EndIf
    GetPersonalQuest().TryToSetStage(419)
    If Storage_MainframeCoresCurrent >= Storage_MainframeCoresMax
        GetPersonalQuest().TryToSetStage(430)
    EndIf
EndFunction

Function OpenSecurityDoor(Bool abSetStage = True)
    ObjectReference doorRef = MSilo_Storage_ExitDoor.GetReference()
    If doorRef != None
        doorRef.Lock(False)
        doorRef.SetOpen(True)
    EndIf
    If abSetStage
        GetPersonalQuest().TryToSetStage(440)
    EndIf
EndFunction

Function UpdateTerminal(ObjectReference akTerminalRef)
    If akTerminalRef != None && Storage_MainframeCoresCurrent >= Storage_MainframeCoresMax
        akTerminalRef.SetValue(MSilo_Storage_SuccessfulBootValue, 1.0)
    EndIf
EndFunction

Function UpdateTerminals()
    Int i = 0
    While i < MSilo_Storage_FacilitiesMainframeTerminals.GetCount()
        UpdateTerminal(MSilo_Storage_FacilitiesMainframeTerminals.GetAt(i))
        i += 1
    EndWhile
EndFunction
