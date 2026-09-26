Event OnQuestInit()
    CurrBreak = 0
    CurrEncWave = 0
    Quest owner = Self as Quest
    EWS = owner as DefaultQuestEncounterWaveScript
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        RegisterForCustomEvent(questTimer, "QuestTimerEnded")
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
    CancelTimer(2)
    CancelTimer(3)
EndEvent

Event B21:QuestTimer.QuestTimerEnded(B21:QuestTimer akSender, Var[] akArgs)
    HandleEventTimerEnd()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        If !EventFinished() && CurrBreak < BreakStages.Length
            SetStage(BreakStages[CurrBreak])
        EndIf
    ElseIf aiTimerID == 2
        Stop()
    ElseIf aiTimerID == 3
        HandleEventTimerEnd()
    EndIf
EndEvent

Bool Function EventFinished()
    Return IsStageDone(SuccessStage) || IsStageDone(FailStage) || IsStopping() || IsStopped()
EndFunction

Function HandleEventTimerEnd()
    If EventFinished()
        Return
    EndIf
    ; Stage 1000's note: the timer ending is a success only once the machines are running and being defended.
    If IsStageDone(700)
        SetStage(SuccessStage)
    Else
        SetStage(FailStage)
    EndIf
EndFunction

Function ResetFactory()
    CancelTimer(1)
    CancelTimer(3)
    CurrBreak = 0
    CurrEncWave = 0
    SetMachineryRunning(False)
    SetMachineryStalled(False)
    ClearDestructionOn(Alias_Machines)
    ClearDestructionOn(FactoryBreakTargets)
EndFunction

Function StartDefendPhase()
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        questTimer.StartQuestTimer(DefendPhaseTimer)
    Else
        StartTimer(DefendPhaseTimer, 3)
    EndIf
    SetMachineryRunning(True)
    AdvanceEncounterWave()
    StartTimer(BreakTimer, 1)
EndFunction

Function BreakFactory()
    If EventFinished()
        Return
    EndIf
    CurrBreak += 1
    SetMachineryStalled(True)
    AdvanceEncounterWave()

    Int pending = 0
    Int index = 0
    While BrokenObjects != None && index < BrokenObjects.Length
        If BrokenObjects[index] != None && !IsStageDone(BrokenObjects[index].RepairStage)
            pending += 1
        EndIf
        index += 1
    EndWhile
    If pending <= 0
        Return
    EndIf

    ; Each break uses a different broken object, chosen at random from those not yet used this run.
    Int chosen = Utility.RandomInt(0, pending - 1)
    index = 0
    While index < BrokenObjects.Length
        BrokenObject candidate = BrokenObjects[index]
        If candidate != None && !IsStageDone(candidate.RepairStage)
            If chosen == 0
                SetStage(candidate.RepairStage)
                Return
            EndIf
            chosen -= 1
        EndIf
        index += 1
    EndWhile
EndFunction

Function ResumeFactory()
    If EventFinished()
        Return
    EndIf
    SetMachineryStalled(False)
    If CurrBreak < BreakStages.Length
        StartTimer(BreakTimer, 1)
    EndIf
EndFunction

Function HandleMachineDestroyed(Int aiDestroyedStage)
    Int destroyed = 0
    Int index = 0
    While HealthBars != None && index < HealthBars.Length
        HealthBar bar = HealthBars[index]
        If bar != None
            If bar.DestroyedStage == aiDestroyedStage
                SetObjectiveFailed(bar.Objective)
            EndIf
            If IsStageDone(bar.DestroyedStage)
                destroyed += 1
            EndIf
        EndIf
        index += 1
    EndWhile
    If HealthBars != None && destroyed >= HealthBars.Length && !EventFinished()
        SetStage(FailStage)
    EndIf
EndFunction

Function FinishEvent()
    CancelTimer(1)
    CancelTimer(3)
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        questTimer.StopQuestTimer()
    EndIf
    If Waves() != None
        EWS.StopAllEncounterWaves(False)
    EndIf
    ; Leave the result on screen briefly; stopping runs the shutdown and cleanup stages.
    StartTimer(5.0, 2)
EndFunction

Function ShutdownFactory()
    CancelTimer(1)
    CancelTimer(3)
    SetMachineryRunning(False)
    SetMachineryStalled(False)
    SetTwoStateCollection(Alias_KlaxonLights, False)
    ClearDestructionOn(FactoryBreakTargets)
    ; Re-enabled so the location-ref-type alias finds the placed originals on the next run.
    SetIngredientDispensersEnabled(True)
EndFunction

DefaultQuestEncounterWaveScript Function Waves()
    ; The startup stage can run before OnQuestInit has cached the sibling script.
    If EWS == None
        Quest owner = Self as Quest
        EWS = owner as DefaultQuestEncounterWaveScript
    EndIf
    Return EWS
EndFunction

Function StartHarassment()
    If Waves() != None && !EventFinished()
        EWS.StartEncounterWave(0)
    EndIf
EndFunction

ObjectReference Function RepairTargetForStage(Int aiRepairStage)
    Int index = 0
    While BrokenObjects != None && index < BrokenObjects.Length
        If BrokenObjects[index] != None && BrokenObjects[index].RepairStage == aiRepairStage
            Return BrokenObjects[index].RefToRepair
        EndIf
        index += 1
    EndWhile
    Return None
EndFunction

Function AdvanceEncounterWave()
    If Waves() == None || EWS.EncounterWaves == None
        Return
    EndIf
    ; Wave 0 is the Liberator harassment started at stage 30; the creature waves escalate through defense and each break.
    If CurrEncWave > 0
        EWS.StopEncounterWave(CurrEncWave)
    EndIf
    If CurrEncWave + 1 < EWS.EncounterWaves.Length
        CurrEncWave += 1
        EWS.StartEncounterWave(CurrEncWave)
    EndIf
EndFunction

Function SetMachineryRunning(Bool abRunning)
    SetMarkerEnabled(FF06_Feed_OnAndRunningMarker, abRunning)
    SetMarkerEnabled(FF06_Feed_MachineSoundsEnableMarker, abRunning)
    SetTwoStateCollection(Alias_Smokestacks, abRunning)
EndFunction

Function SetMachineryStalled(Bool abStalled)
    If FF06_Feed_MachineryStalled != None
        If abStalled
            FF06_Feed_MachineryStalled.SetValue(1.0)
        Else
            FF06_Feed_MachineryStalled.SetValue(0.0)
        EndIf
    EndIf
    SetMarkerEnabled(FF06_Feed_FactoryBreakAlarm, abStalled)
    SetTwoStateCollection(Alias_KlaxonLights, abStalled)
    If IsStageDone(700) && !EventFinished()
        SetMarkerEnabled(FF06_Feed_OnAndRunningMarker, !abStalled)
        SetMarkerEnabled(FF06_Feed_MachineSoundsEnableMarker, !abStalled)
        SetTwoStateCollection(Alias_Smokestacks, !abStalled)
    EndIf
EndFunction

Function SetMarkerEnabled(ObjectReference akMarker, Bool abEnabled)
    If akMarker == None
        Return
    EndIf
    If abEnabled
        akMarker.EnableNoWait()
    Else
        akMarker.DisableNoWait()
    EndIf
EndFunction

Function SetTwoStateCollection(RefCollectionAlias akCollection, Bool abOpen)
    Int index = 0
    While akCollection != None && index < akCollection.GetCount()
        Default2StateActivator twoState = akCollection.GetAt(index) as Default2StateActivator
        If twoState != None
            twoState.SetOpenNoWait(abOpen)
        EndIf
        index += 1
    EndWhile
EndFunction

Function ClearDestructionOn(RefCollectionAlias akCollection)
    Int index = 0
    While akCollection != None && index < akCollection.GetCount()
        ObjectReference target = akCollection.GetAt(index)
        If target != None
            target.ClearDestruction()
        EndIf
        index += 1
    EndWhile
EndFunction

RefCollectionAlias Function IngredientCollectionFor(Form akIngredient, RefCollectionAlias akMeat, RefCollectionAlias akStock, RefCollectionAlias akVegetables)
    If akIngredient == FF06_Feed_Ingredient_Meat
        Return akMeat
    ElseIf akIngredient == FF06_Feed_Ingredient_Stock
        Return akStock
    ElseIf akIngredient == FF06_Feed_Ingredient_Vegatables
        Return akVegetables
    EndIf
    Return None
EndFunction

Function SetIngredientDispensersEnabled(Bool abEnabled)
    Int index = 0
    While Alias_ItemDispensers != None && index < Alias_ItemDispensers.GetCount()
        ObjectReference dispenser = Alias_ItemDispensers.GetAt(index)
        If dispenser != None && dispenser.GetContainer() == None
            If abEnabled
                dispenser.EnableNoWait()
            Else
                dispenser.DisableNoWait()
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function SpawnIngredients(RefCollectionAlias akMeat, RefCollectionAlias akStock, RefCollectionAlias akVegetables)
    ; FO76 dispensers hand every player a fresh item; placed originals stay hidden as spawn points so each run gets a full set.
    Actor playerRef = Game.GetPlayer()
    Int index = 0
    While playerRef != None && Alias_ItemDispensers != None && index < Alias_ItemDispensers.GetCount()
        ObjectReference dispenser = Alias_ItemDispensers.GetAt(index)
        If dispenser != None && dispenser.GetContainer() == None
            Form ingredientForm = dispenser.GetBaseObject()
            RefCollectionAlias collection = IngredientCollectionFor(ingredientForm, akMeat, akStock, akVegetables)
            If collection != None
                ObjectReference spawned = playerRef.PlaceAtMe(ingredientForm, 1, False, True, False)
                If spawned != None
                    spawned.MoveTo(dispenser)
                    spawned.EnableNoWait()
                    collection.AddRef(spawned)
                EndIf
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function DeleteUncollectedIngredients(RefCollectionAlias akCollection)
    Int index = 0
    While akCollection != None && index < akCollection.GetCount()
        ObjectReference ingredientRef = akCollection.GetAt(index)
        If ingredientRef != None && ingredientRef.GetContainer() == None
            ingredientRef.DisableNoWait()
            ingredientRef.Delete()
        EndIf
        index += 1
    EndWhile
EndFunction
