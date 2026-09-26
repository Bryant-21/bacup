Event OnQuestInit()
    Parent.OnQuestInit()
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    ConfigureLocalTerminals()
    TryStartNestEncounter()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    TryStartNestEncounter()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        ConfigureLocalTerminals()
        TryStartNestEncounter()
    EndIf
EndEvent

Event ObjectReference.OnLoad(ObjectReference akSender)
    If BoneMealContainer != None && akSender == BoneMealContainer.GetReference()
        TryStartNestEncounter()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 330
        TryStartNestEncounter()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(330)
    UnregisterForAllRemoteEvents()
EndEvent

Function TryStartNestEncounter()
    If !IsRunning() || IsCompleted() || !IsStageDone(300) || IsStageDone(330) || IsStageDone(340)
        CancelTimer(330)
        Return
    EndIf
    If B21NestStarting
        Return
    EndIf
    ObjectReference nest
    If BoneMealContainer != None
        nest = BoneMealContainer.GetReference()
    EndIf
    If nest == None
        StartTimer(5.0, 330)
        Return
    EndIf
    RegisterForRemoteEvent(nest, "OnLoad")
    If !nest.Is3DLoaded()
        Return
    EndIf
    B21NestStarting = True
    Quest organicQuest = Self
    B21:LocalEncounterMaterializer materializer = organicQuest as B21:LocalEncounterMaterializer
    DefaultQuestEncounterWaveScript waves = organicQuest as DefaultQuestEncounterWaveScript
    If materializer == None || waves == None || waves.EncounterWaves == None || waves.EncounterWaves.Length == 0
        B21NestStarting = False
        StartTimer(5.0, 330)
        Return
    EndIf
    materializer.PrepareEligibleWaves()
    RefCollectionAlias enemies = GetAlias(50) as RefCollectionAlias
    If !materializer.HasPreparedWave(0) || enemies == None || enemies.GetCount() == 0
        B21NestStarting = False
        StartTimer(5.0, 330)
        Return
    EndIf
    Actor deathclaw = enemies.GetAt(0) as Actor
    If deathclaw != None && IsRunning() && !IsStageDone(340)
        deathclaw.MoveTo(nest, waves.EncounterWaves[0].SpawnAreaRadiusMin, 0.0, 0.0)
        deathclaw.MoveToNearestNavmeshLocation()
        waves.StartLocalEncounterWave(0)
    EndIf
    B21NestStarting = False
    CancelTimer(330)
EndFunction

Function ConfigureLocalTerminals()
    DefaultAliasSetStageOnMenuItemRun terminalAlias = GetAlias(16) as DefaultAliasSetStageOnMenuItemRun
    If terminalAlias != None && terminalAlias.ValueSet != None
        Int index = 0
        While index < terminalAlias.ValueSet.Length
            DefaultAliasSetStageOnMenuItemRun:ValueData entry = terminalAlias.ValueSet[index]
            If entry.iStageToSet == 460
                entry.iPrereqStage = 450
                entry.iShutdownStage = 1000
            ElseIf entry.iStageToSet == 600
                entry.iPrereqStage = 500
                entry.iShutdownStage = 1000
            EndIf
            terminalAlias.ValueSet[index] = entry
            index += 1
        EndWhile
        terminalAlias.OnAliasInit()
    EndIf
    Quest organicQuest = Self
    Fragments:Quests:QF_SFM04_Organic_0010AE02 fragments = organicQuest as Fragments:Quests:QF_SFM04_Organic_0010AE02
    If fragments != None
        fragments.ReconcileChemistryGlobals()
        fragments.ReconcileDyerSounds()
        fragments.ReconcileTrackerRadio()
    EndIf
EndFunction
