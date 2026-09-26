Event OnQuestInit()
    B21EventFinishing = False
    CampfireCurrPercent = StartPercent
    CacheEventScripts()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 46483
        CleanupEvent()
        If IsRunning()
            Stop()
        EndIf
    EndIf
EndEvent

; Quest script state survives a stop, so the ending is cleared here for the next run.
Event OnQuestShutdown()
    CancelTimer(46483)
    CleanupEvent()
    ChosenTale = None
EndEvent

Function CacheEventScripts()
    Quest owner = Self as Quest
    If EWS == None
        EWS = owner as DefaultQuestEncounterWaveScript
    EndIf
    If DEQ == None
        DEQ = owner as DefaultEventQuest
    EndIf
    If CampfireScript == None && Alias_Activator_Campfire != None
        CampfireScript = Alias_Activator_Campfire as Quests:E01C_Tales:Dark:CampfireScript
    EndIf
    If KindlingScript == None && Alias_Markers_Kindling != None
        KindlingScript = Alias_Markers_Kindling as Quests:E01C_Tales:Dark:KindlingSpawner
    EndIf
EndFunction

Function EnsureTaleSelected()
    If ChosenTale != None || TaleEndings == None
        Return
    EndIf
    Int[] candidates = New Int[0]
    Int index = 0
    While index < TaleEndings.Length
        If TaleEndings[index] != None && TaleEndings[index].Enabled
            If IsStageDone(TaleEndings[index].StageToSet)
                ChosenTale = TaleEndings[index]
                Return
            EndIf
            candidates.Add(index)
        EndIf
        index += 1
    EndWhile
    If candidates.Length == 0
        Return
    EndIf
    ChosenTale = TaleEndings[candidates[Utility.RandomInt(0, candidates.Length - 1)]]
    If ChosenTale.StageToSet > 0 && !IsStageDone(ChosenTale.StageToSet)
        SetStage(ChosenTale.StageToSet)
    EndIf
EndFunction

; A console-set ending stage overrides the random pick until the boss fight has begun.
Function SelectTale(Int aiStage)
    If TaleEndings == None || IsStageDone(900)
        Return
    EndIf
    Int index = TaleEndings.FindStruct("StageToSet", aiStage)
    If index >= 0
        ChosenTale = TaleEndings[index]
    EndIf
EndFunction

Function StartWaveByID(String asIDString)
    CacheEventScripts()
    If EWS != None && EWS.FindEncounterWaveIndex(asIDString) >= 0
        EWS.StartEncounterWaveByID(asIDString)
    EndIf
EndFunction

Function SetEventVariable(String asName, Float afValue)
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables != None
        variables.SetVariable(asName, afValue)
    EndIf
EndFunction

Function BeginShadowsActivity()
    StartWaveByID("Ambient Mosquitoes")
    If Alias_ShadowMarkers == None || Alias_Shadows == None || E01C_Tales_Dark_DestructibleShadow == None
        SetStage(350)
        Return
    EndIf
    If Alias_Shadows.GetCount() == 0
        Int index = 0
        Int markerCount = Alias_ShadowMarkers.GetCount()
        While index < markerCount
            ObjectReference marker = Alias_ShadowMarkers.GetAt(index)
            If marker != None
                ObjectReference shadow = marker.PlaceAtMe(E01C_Tales_Dark_DestructibleShadow, 1, True)
                If shadow != None
                    Alias_Shadows.AddRef(shadow)
                EndIf
            EndIf
            index += 1
        EndWhile
    EndIf
    Quests:E01C_Tales:Dark:ShadowsScript shadowsScript = Alias_Shadows as Quests:E01C_Tales:Dark:ShadowsScript
    If shadowsScript != None
        shadowsScript.ResetShadowCount()
    EndIf
    If Alias_Shadows.GetCount() == 0
        SetStage(350)
    EndIf
EndFunction

Function BeginKindlingGathering()
    CacheEventScripts()
    If KindlingScript != None
        KindlingScript.StartSpawning()
    EndIf
EndFunction

Function PlaceEvidence()
    If Alias_Misc_PlacedEvidence == None
        SetStage(850)
        Return
    EndIf
    If Alias_Misc_PlacedEvidence.GetCount() == 0
        EnsureTaleSelected()
        Int index = 0
        While GenericEvidence != None && index < GenericEvidence.Length
            Evidence row = GenericEvidence[index]
            If row != None
                PlaceEvidenceItem(row.ItemToPlace, row.MarkerToPlaceAt, row.AliasToFill)
            EndIf
            index += 1
        EndWhile
        If ChosenTale != None
            PlaceEvidenceItem(ChosenTale.ItemToPlace1, ChosenTale.MarkerToPlaceAt1, ChosenTale.AliasToFill1)
            PlaceEvidenceItem(ChosenTale.ItemToPlace2, ChosenTale.MarkerToPlaceAt2, ChosenTale.AliasToFill2)
            PlaceEvidenceItem(ChosenTale.ItemToPlace3, ChosenTale.MarkerToPlaceAt3, ChosenTale.AliasToFill3)
        EndIf
    EndIf
    ; Nothing could be placed, so there is nothing to find; continue the tale instead of stalling.
    If Alias_Misc_PlacedEvidence.GetCount() == 0
        SetStage(850)
    EndIf
EndFunction

Function PlaceEvidenceItem(MiscObject akItem, ReferenceAlias akMarkerAlias, ReferenceAlias akAliasToFill)
    If akItem == None || akMarkerAlias == None
        Return
    EndIf
    ObjectReference marker = akMarkerAlias.GetReference()
    If marker == None
        Return
    EndIf
    ObjectReference item = marker.PlaceAtMe(akItem, 1, True)
    If item == None
        Return
    EndIf
    If akAliasToFill != None
        akAliasToFill.ForceRefTo(item)
    EndIf
    Alias_Misc_PlacedEvidence.AddRef(item)
EndFunction

; Objective 50's per-item compass targets are conditioned on these area stages.
Function RefreshEvidenceTargets()
    If IsObjectiveDisplayed(50) && !IsObjectiveCompleted(50)
        SetObjectiveDisplayed(50, True, False)
    EndIf
EndFunction

Function StartBossFight()
    CacheEventScripts()
    EnsureTaleSelected()
    If ChosenTale != None
        Int tale = ChosenTale.StageToSet
        If tale == 110
            StartWaveByID("Wolves")
        ElseIf tale == 120
            SetEventVariable("WaveTotal", 3.0)
            SetEventVariable("WaveCurr", 1.0)
        ElseIf tale == 130
            StartWaveByID("Flatwoods Monster Clones")
        EndIf
        If EWS != None
            EWS.StartEncounterWave(ChosenTale.EnemyWave)
        EndIf
        If tale == 130 && Alias_Actors_FlatwoodsClones != None
            Quests:E01C_Tales:Dark:FlatwoodsClonesScript clonesScript = Alias_Actors_FlatwoodsClones as Quests:E01C_Tales:Dark:FlatwoodsClonesScript
            If clonesScript != None
                clonesScript.BeginCloneTracking()
            EndIf
        EndIf
        SetObjectiveDisplayed(ChosenTale.EnemyObjective, True, True)
    EndIf
    SetObjectiveDisplayed(Obj_Campfire, True, True)
    If CampfireScript != None
        CampfireScript.StartDrain()
    EndIf
EndFunction

Function AdvanceInsectWave(Int aiStage)
    If ChosenTale == None || ChosenTale.StageToSet != 120 || IsStageDone(9000) || IsStageDone(Stage_CampfireWentOut)
        Return
    EndIf
    CacheEventScripts()
    If aiStage == 921
        SetEventVariable("WaveCurr", 2.0)
        StartWaveByID("Bug Swarm: Hard")
        If CampfireScript != None
            CampfireScript.SetReductionPercent(ReductionPercentWave2)
        EndIf
    ElseIf aiStage == 922
        SetEventVariable("WaveCurr", 3.0)
        StartWaveByID("Bug Swarm: Very Hard")
        StartWaveByID("Radscorpion Boss")
        If CampfireScript != None
            CampfireScript.SetReductionPercent(ReductionPercentWave3)
        EndIf
    EndIf
EndFunction

Function DisableCampfireMechanic()
    CacheEventScripts()
    If CampfireScript != None
        CampfireScript.StopDrain()
    EndIf
EndFunction

Function AddKindlingToCampfire()
    CampfireCurrPercent = CampfireCurrPercent + AdditionPercent
    UpdateCampfireProgressBar()
EndFunction

Function UpdateCampfireProgressBar()
    If CampfireCurrPercent > 1.0
        CampfireCurrPercent = 1.0
    ElseIf CampfireCurrPercent < 0.0
        CampfireCurrPercent = 0.0
    EndIf
    CacheEventScripts()
    If CampfireScript != None
        CampfireScript.UpdateFireState(CampfireCurrPercent, LowWarningPercent)
    EndIf
    If CampfireCurrPercent <= 0.0 && IsStageDone(900) && !IsStageDone(9000) && !IsStageDone(Stage_CampfireWentOut)
        SetStage(Stage_CampfireWentOut)
    EndIf
EndFunction

Function ResolveOpenObjective(Int aiObjective, Bool abFailed)
    If !IsObjectiveDisplayed(aiObjective) || IsObjectiveCompleted(aiObjective) || IsObjectiveFailed(aiObjective)
        Return
    EndIf
    If abFailed
        SetObjectiveFailed(aiObjective, True)
    Else
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FinishEvent(Bool abFailed)
    If B21EventFinishing
        Return
    EndIf
    B21EventFinishing = True
    CacheEventScripts()
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        questTimer.StopQuestTimer()
    EndIf
    If CampfireScript != None
        CampfireScript.StopDrain()
    EndIf
    If KindlingScript != None
        KindlingScript.StopSpawning()
    EndIf
    If Alias_Actors_FlatwoodsClones != None
        Quests:E01C_Tales:Dark:FlatwoodsClonesScript clonesScript = Alias_Actors_FlatwoodsClones as Quests:E01C_Tales:Dark:FlatwoodsClonesScript
        If clonesScript != None
            clonesScript.StopCloneTracking()
        EndIf
    EndIf
    If EWS != None
        EWS.StopAllEncounterWaves(False)
    EndIf
    ResolveOpenObjective(10, abFailed)
    ResolveOpenObjective(20, abFailed)
    ResolveOpenObjective(25, abFailed)
    ResolveOpenObjective(30, abFailed)
    ResolveOpenObjective(40, abFailed)
    ResolveOpenObjective(50, abFailed)
    ResolveOpenObjective(60, abFailed)
    ResolveOpenObjective(Obj_Campfire, abFailed)
    ResolveOpenObjective(70, abFailed)
    ResolveOpenObjective(85, abFailed)
    ResolveOpenObjective(90, abFailed)
    ResolveOpenObjective(100, abFailed)
    ; The delay lets stage rewards and the completion party crasher run before shutdown.
    CancelTimer(46483)
    StartTimer(10.0, 46483)
EndFunction

Function CleanupEvent()
    CacheEventScripts()
    If KindlingScript != None
        KindlingScript.CleanupKindling()
    EndIf
    If CampfireScript != None
        CampfireScript.StopDrain()
        CampfireScript.ExtinguishCampfire()
    EndIf
    DeleteWorldRefs(Alias_Shadows, False)
    DeleteWorldRefs(Alias_Misc_PlacedEvidence, True)
EndFunction

; Evidence already carried by the player stays for DefaultQuestRemovePlayersScript's item cleanup.
Function DeleteWorldRefs(RefCollectionAlias akCollection, Bool abOnlyUncollected)
    If akCollection == None
        Return
    EndIf
    Int index = akCollection.GetCount() - 1
    While index >= 0
        ObjectReference worldRef = akCollection.GetAt(index)
        If worldRef != None && (!abOnlyUncollected || worldRef.GetContainer() == None)
            akCollection.RemoveRef(worldRef)
            worldRef.DisableNoWait()
            worldRef.Delete()
        EndIf
        index -= 1
    EndWhile
EndFunction
