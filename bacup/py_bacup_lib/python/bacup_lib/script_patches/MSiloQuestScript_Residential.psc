Event OnQuestInit()
    Initialize()
EndEvent

Function Initialize()
    Quest siloQuest = Self as Quest
    MSiloResidential = Self
    MSiloMain = siloQuest as MSiloQuestScript_Main
    MSiloControl = siloQuest as MSiloQuestScript_Control
    MSiloStorage = siloQuest as MSiloQuestScript_Storage
    MSiloOperations = siloQuest as MSiloQuestScript_Operations
    MSiloReactor = siloQuest as MSiloQuestScript_Reactor
    CONST_Generic_ShowControlRoomObjectiveStage = 10
    CONST_Generic_HideControlRoomObjectiveStage = 11
    CONST_Residential_EnteredAreaStage = 100
    CONST_Residential_ReadRegistrationTerminalStage = 110
    CONST_Residential_ReadFabricationTerminalStage = 110
    CONST_Residential_GotPrewarIDCardStage = 120
    CONST_Residential_WipedIDCardStage = 140
    CONST_Residential_GotBiometricDataStage = 150
    CONST_Residential_FabricatedIDCardStage = 160
    CONST_Residential_RegisteredIDCardStage = 180
    CONST_Residential_GiveMidquestReward = 198
    isEventEnabled = True
    hasInitialized = True
EndFunction

MSiloPersonalQuestScript Function GetPersonalQuest()
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    If personalQuest != None && !personalQuest.IsRunning()
        personalQuest.Start()
    EndIf
    Return personalQuest as MSiloPersonalQuestScript
EndFunction

Function BeginBiometricEnrollment()
    MSiloPersonalQuestScript personal = GetPersonalQuest()
    personal.TryToSetStage(110)
    personal.TryToSetStage(120)
EndFunction

Function HandleIDCardActivation(ObjectReference akCard, ObjectReference akActivator)
    If akActivator != Game.GetPlayer()
        Return
    EndIf
    akActivator.AddItem(MSilo_Residential_PrewarIDCard, 1, True)
    akCard.Disable()
    MSiloPersonalQuestScript personal = GetPersonalQuest()
    personal.TryToSetStage(120)
    personal.TryToSetStage(130)
EndFunction

Function FabricateAndAuthorizeID()
    Actor playerRef = Game.GetPlayer()
    playerRef.RemoveItem(MSilo_Residential_PrewarIDCard, playerRef.GetItemCount(MSilo_Residential_PrewarIDCard), True)
    playerRef.RemoveItem(MSilo_Residential_BlankIDCard, playerRef.GetItemCount(MSilo_Residential_BlankIDCard), True)
    playerRef.RemoveItem(MSilo_Residential_BiometricData, playerRef.GetItemCount(MSilo_Residential_BiometricData), True)
    playerRef.RemoveItem(MSilo_Residential_PlayerIDCard, playerRef.GetItemCount(MSilo_Residential_PlayerIDCard), True)
    If playerRef.GetItemCount(MSilo_Residential_AuthorizedPlayerIDCard) < 1
        playerRef.AddItem(MSilo_Residential_AuthorizedPlayerIDCard, 1, True)
    EndIf

    MSiloPersonalQuestScript personal = GetPersonalQuest()
    personal.TryToSetStage(110)
    personal.TryToSetStage(120)
    personal.TryToSetStage(140)
    personal.TryToSetStage(150)
    personal.TryToSetStage(160)
    personal.TryToSetStage(180)
    SetLaserGridsOpen(True)
EndFunction

Function SetLaserGridsOpen(Bool abOpen)
    Int i = 0
    While i < MSilo_Residential_LaserGrids.GetCount()
        MSiloLaserGridScript grid = MSilo_Residential_LaserGrids.GetAt(i) as MSiloLaserGridScript
        If grid != None
            grid.SetOpen(abOpen)
        EndIf
        i += 1
    EndWhile
EndFunction
