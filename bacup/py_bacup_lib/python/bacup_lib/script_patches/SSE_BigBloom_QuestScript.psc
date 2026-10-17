; FO76's server ran The Big Bloom. Accepting Susan's job (her scene's SendOff line, action 4) sets 125; the
; basket sets 500/550/600 per flower type; burning sets 750 (BurntOvergrownScript). This script owns the rest of
; the chain: flower attackers, mine re-arming, the burn waves, the bee finale and Beezlebub.
; B21:ObjectiveTimers owns 15 -> 127, the flower time limit and 80 -> 780; B21:QuestTimer owns the event timer.
Function StartEvent()
    Quest owner = Self as Quest
    cachedDefaultEventQuest = owner as DefaultEventQuest
    DefaultQuestEncounterWaveScript waves = EWS()
    If waves != None
        RegisterForCustomEvent(waves, "WaveComplete")
    EndIf
    Scene susanScene = Game.GetFormFromFile(0x79AC1B, "SeventySix.esm") as Scene
    If susanScene != None
        RegisterForRemoteEvent(susanScene, "OnAction")
    EndIf
    ObjectReference boomBox = AliasRef(26)
    If boomBox != None
        RegisterForRemoteEvent(boomBox, "OnActivate")
    EndIf
    EnableAll(Alias_Marigolds)
    EnableAll(Alias_Tulips)
    EnableAll(Alias_Pansies)
    EnableAll(Alias_LandmineTriggers)
    ResetFinale()
    If BigBloomWeather != None
        BigBloomWeather.SetActive(True, True)
    EndIf
    If CSDuckMusic != None
        CSDuckMusic.Push()
    EndIf
    StartTimer(landmineTimerduration as Float, landmineTimerID)
EndFunction

DefaultQuestEncounterWaveScript Function EWS()
    Return (Self as Quest) as DefaultQuestEncounterWaveScript
EndFunction

ObjectReference Function AliasRef(Int aiAliasID)
    ReferenceAlias found = GetAlias(aiAliasID) as ReferenceAlias
    If found == None
        Return None
    EndIf
    Return found.GetReference()
EndFunction

Function EnableAll(RefCollectionAlias akCollection)
    If akCollection == None
        Return
    EndIf
    Int index = 0
    While index < akCollection.GetCount()
        ObjectReference target = akCollection.GetAt(index)
        If target != None && target.IsDisabled()
            target.Enable()
        EndIf
        index += 1
    EndWhile
EndFunction

Function ResetFinale()
    Int aliasID = 31
    While aliasID <= 32
        RefCollectionAlias finale = GetAlias(aliasID) as RefCollectionAlias
        If finale != None
            Int index = 0
            While index < finale.GetCount()
                ObjectReference target = finale.GetAt(index)
                If target != None
                    target.Disable()
                    Actor bee = target as Actor
                    If bee != None && bee.IsDead()
                        bee.Reset()
                        bee.Disable()
                    EndIf
                EndIf
                index += 1
            EndWhile
        EndIf
        aliasID += 1
    EndWhile
    SetBasketFull(False)
EndFunction

Function SetBasketFull(Bool abFull)
    ObjectReference empty = AliasRef(11)
    ObjectReference full = AliasRef(29)
    If full != None && empty != None && full != empty
        If abFull
            full.Enable()
            empty.Disable()
        Else
            full.Disable()
            empty.Enable()
        EndIf
    EndIf
EndFunction

Event Scene.OnAction(Scene akSender, Int auiActionID, ReferenceAlias akAlias)
    If auiActionID == 4 && IsStageDone(questStartStage) && !IsStageDone(125)
        SetStage(125)
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && IsStageDone(AllFlowersCollectedStage) && !IsStageDone(720)
        SetStage(720)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 127
        StartTimer(flowerPickingTimerWarningVal as Float, flowerWarningTimerID)
    ElseIf auiStageID == 130
        StartWave(0)
    ElseIf auiStageID == radliliesCollectedStage || auiStageID == crystalcupsCollectedStage || auiStageID == carnalWeepersCollecedStage
        If IsStageDone(radliliesCollectedStage) && IsStageDone(crystalcupsCollectedStage) && IsStageDone(carnalWeepersCollecedStage)
            SetStage(AllFlowersCollectedStage)
        EndIf
    ElseIf auiStageID == AllFlowersCollectedStage
        CancelTimer(flowerWarningTimerID)
        StopWaves()
        SetBasketFull(True)
    ElseIf auiStageID == 710
        Int wave = 7
        While wave <= 10
            StartWave(wave)
            wave += 1
        EndWhile
    ElseIf auiStageID == 750
        StartTimer(3.0, 50)
    ElseIf auiStageID == 770
        StopWaves()
        StartTimer(beezlebabyTimerDuration as Float, beezlebabyTimerID)
    ElseIf auiStageID == 800
        StartWave(11)
        StartWave(12)
        StartWave(13)
    ElseIf auiStageID == questCompletionStage || auiStageID == questFailureStage
        StopWaves()
    EndIf
EndEvent

Function StartWave(Int aiWave)
    DefaultQuestEncounterWaveScript waves = EWS()
    If waves != None
        waves.StartEncounterWave(aiWave)
    EndIf
EndFunction

Function StopWaves()
    DefaultQuestEncounterWaveScript waves = EWS()
    If waves != None
        waves.StopAllEncounterWaves()
    EndIf
EndFunction

Event DefaultQuestEncounterWaveScript.WaveComplete(DefaultQuestEncounterWaveScript akSender, Var[] akArgs)
    If akArgs.Length == 0 || !IsStageDone(130) || IsStageDone(AllFlowersCollectedStage)
        Return
    EndIf
    Int wave = akArgs[0] as Int
    If wave >= 0 && wave <= 6
        StartWave((wave + 1) % 7)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If !IsRunning() || IsStageDone(questCompletionStage) || IsStageDone(questFailureStage)
        Return
    EndIf
    If aiTimerID == landmineTimerID
        EnableAll(Alias_LandmineTriggers)
        StartTimer(landmineTimerduration as Float, landmineTimerID)
    ElseIf aiTimerID == flowerWarningTimerID
        If !IsStageDone(AllFlowersCollectedStage) && FlowerPickingWarning != None
            FlowerPickingWarning.Show()
        EndIf
    ElseIf aiTimerID == 50
        CheckOvergrownCleared()
    ElseIf aiTimerID == beezlebabyTimerID
        SetStage(spawnFinaleBeezlebabiesStage)
    EndIf
EndEvent

Function CheckOvergrownCleared()
    If IsStageDone(770)
        Return
    EndIf
    DefaultQuestEncounterWaveScript waves = EWS()
    RefCollectionAlias overgrown = GetAlias(4) as RefCollectionAlias
    Int living = 0
    Bool spawning = False
    If waves != None && overgrown != None
        living = waves.CountLivingCollectionActors(overgrown)
        Int wave = 7
        While wave <= 10
            spawning = spawning || waves.IsEncounterWaveSpawning(wave)
            wave += 1
        EndWhile
    EndIf
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable("Overgrown", living as Float)
    EndIf
    If living == 0 && !spawning
        SetStage(770)
    Else
        StartTimer(3.0, 50)
    EndIf
EndFunction

Event OnQuestShutdown()
    CancelTimer(landmineTimerID)
    CancelTimer(flowerWarningTimerID)
    CancelTimer(beezlebabyTimerID)
    CancelTimer(50)
    StopWaves()
    If BigBloomWeather != None
        Weather.ReleaseOverride()
    EndIf
    If CSDuckMusic != None
        CSDuckMusic.Remove()
    EndIf
    Scene susanScene = Game.GetFormFromFile(0x79AC1B, "SeventySix.esm") as Scene
    If susanScene != None
        UnregisterForRemoteEvent(susanScene, "OnAction")
    EndIf
    DefaultQuestEncounterWaveScript waves = EWS()
    If waves != None
        UnregisterForCustomEvent(waves, "WaveComplete")
    EndIf
EndEvent
