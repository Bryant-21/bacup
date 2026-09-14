Event OnQuestInit()
    initialized = True
    questPlayer = Game.GetPlayer()
    parentQuest = MSilo
EndEvent

Bool Function PrepareSiloAliases(Location akLocation)
    Actor player = Game.GetPlayer()
    Quest managerQuest = MSilo
    Quest personalQuest = Self as Quest
    If player == None || managerQuest == None || personalQuest == None || akLocation == None
        Return False
    EndIf
    If !personalQuest.IsRunning() && (personalQuest.IsCompleted() || personalQuest.IsStageDone(1000))
        personalQuest.Reset()
    EndIf

    MSiloQuestScript_Main manager = managerQuest as MSiloQuestScript_Main
    If manager == None
        Return False
    EndIf
    manager.SelectLocation(akLocation)
    LocationAlias managerLocationAlias = managerQuest.GetAlias(2) as LocationAlias
    Location canonicalLocation = akLocation
    If managerLocationAlias != None && managerLocationAlias.GetLocation() != None
        canonicalLocation = managerLocationAlias.GetLocation()
    EndIf

    ReferenceAlias playerAlias = personalQuest.GetAlias(0) as ReferenceAlias
    If playerAlias != None
        playerAlias.ForceRefTo(player)
    EndIf
    MSiloLocation.ForceLocationTo(canonicalLocation)

    questPlayer = player
    questLocation = canonicalLocation
    parentQuest = managerQuest
    Return True
EndFunction

Bool Function EnsureSiloStarted(Location akLocation)
    Quest managerQuest = MSilo
    Quest personalQuest = Self as Quest
    If managerQuest == None || personalQuest == None || akLocation == None
        Return False
    EndIf
    If !PrepareSiloAliases(akLocation)
        Return False
    EndIf
    Return (managerQuest.IsRunning() || managerQuest.IsCompleted()) && (personalQuest.IsRunning() || personalQuest.IsCompleted())
EndFunction

Function BeginSilo(Location akLocation)
    questPlayer = Game.GetPlayer()
    questLocation = akLocation
    MSiloLocation.ForceLocationTo(akLocation)
    If !IsStageDone(10)
        SetStage(10)
    EndIf
EndFunction

Function TryToSetStage(Int aiStage)
    Quest personalQuest = Self as Quest
    If personalQuest == None || !personalQuest.IsRunning()
        Return
    EndIf
    If !IsStageDone(aiStage)
        SetStage(aiStage)
    EndIf
EndFunction

Function CompleteObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective)
        SetObjectiveCompleted(aiObjective)
    EndIf
EndFunction

Function HideObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective)
        SetObjectiveDisplayed(aiObjective, False)
    EndIf
EndFunction

Function HandleStage(Int aiStage)
    lastStageSet = aiStage
    If aiStage == 10
        SetObjectiveDisplayed(10)
    ElseIf aiStage == 11
        HideObjective(10)
    ElseIf aiStage == 19
        CompleteObjective(10)
    ElseIf aiStage == 100
        SetObjectiveDisplayed(100)
    ElseIf aiStage == 110
        CompleteObjective(100)
        SetObjectiveDisplayed(110)
    ElseIf aiStage == 120
        SetObjectiveDisplayed(120)
    ElseIf aiStage == 130
        CompleteObjective(120)
        SetObjectiveDisplayed(121)
    ElseIf aiStage == 140
        CompleteObjective(121)
        SetObjectiveDisplayed(122)
    ElseIf aiStage == 150
        CompleteObjective(122)
        SetObjectiveDisplayed(160)
    ElseIf aiStage == 160
        CompleteObjective(160)
        SetObjectiveDisplayed(170)
    ElseIf aiStage == 180
        CompleteObjective(170)
        HideObjective(110)
        (MSilo as MSiloQuestScript_Residential).SetLaserGridsOpen(True)
        TryToSetStage(198)
    ElseIf aiStage == 198
        TryToSetStage(199)
    ElseIf aiStage == 200
        SetObjectiveDisplayed(210)
    ElseIf aiStage == 210
        CompleteObjective(210)
        (MSilo as MSiloQuestScript_Reactor).OpenSecurityDoors()
    ElseIf aiStage == 220
        SetObjectiveDisplayed(220)
        SetObjectiveDisplayed(221)
    ElseIf aiStage == 230
        CompleteObjective(220)
        CompleteObjective(221)
        SetObjectiveDisplayed(230)
        SetObjectiveDisplayed(231)
    ElseIf aiStage == 240
        CompleteObjective(230)
        CompleteObjective(231)
        SetObjectiveDisplayed(240)
    ElseIf aiStage == 250
        CompleteObjective(240)
        (MSilo as MSiloQuestScript_Reactor).OpenSecurityDoors()
        TryToSetStage(298)
    ElseIf aiStage == 298
        TryToSetStage(299)
    ElseIf aiStage == 300
        SetObjectiveDisplayed(310)
    ElseIf aiStage == 320
        CompleteObjective(310)
        (MSilo as MSiloQuestScript_Operations).SetLaserGridsOpen(True)
        TryToSetStage(398)
    ElseIf aiStage == 398
        TryToSetStage(399)
    ElseIf aiStage == 400
        SetObjectiveDisplayed(410)
    ElseIf aiStage == 419
        CompleteObjective(410)
        SetObjectiveDisplayed(420)
        SetObjectiveDisplayed(421)
        SetObjectiveDisplayed(422)
    ElseIf aiStage == 430
        CompleteObjective(420)
        CompleteObjective(421)
        CompleteObjective(422)
        SetObjectiveDisplayed(430)
    ElseIf aiStage == 440
        CompleteObjective(430)
        (MSilo as MSiloQuestScript_Storage).OpenSecurityDoor(False)
        TryToSetStage(498)
    ElseIf aiStage == 498
        TryToSetStage(499)
    ElseIf aiStage == 500
        SetObjectiveDisplayed(510)
    ElseIf aiStage == 520
        CompleteObjective(510)
        SetObjectiveDisplayed(520)
        SetObjectiveDisplayed(521)
        SetObjectiveDisplayed(522)
    ElseIf aiStage == 530
        CompleteObjective(520)
        CompleteObjective(521)
        CompleteObjective(522)
        CompleteObjective(10)
        (MSilo as MSiloQuestScript_Control).CompleteLaunchPrep()
        TryToSetStage(598)
    ElseIf aiStage == 598
        TryToSetStage(599)
    ElseIf aiStage == 1000
        CompleteAllObjectives()
        Stop()
    EndIf
EndFunction
