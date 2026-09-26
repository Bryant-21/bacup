FF06_Feed_QuestScript Function FeedScript()
    Quest owner = Self as Quest
    Return owner as FF06_Feed_QuestScript
EndFunction

Function SetAliasRefEnabled(ReferenceAlias akAlias, Bool abEnabled)
    If akAlias == None || akAlias.GetReference() == None
        Return
    EndIf
    If abEnabled
        akAlias.GetReference().EnableNoWait()
    Else
        akAlias.GetReference().DisableNoWait()
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

Function TryAdvanceToActivation()
    If IsStageDone(100) && IsStageDone(200) && IsStageDone(300) && IsStageDone(400) && !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function ShowDiagnoseObjective()
    ; Every break reuses objective 70, so reopen it before showing it again.
    SetObjectiveCompleted(70, False)
    SetObjectiveDisplayed(70, True, True)
EndFunction

Function StartRepair(ReferenceAlias akRepairAlias, Int aiDiagnoseStage, Int aiRepairObjective)
    SetObjectiveCompleted(70)
    If akRepairAlias != None
        ObjectReference target = FeedScript().RepairTargetForStage(aiDiagnoseStage)
        If target != None
            akRepairAlias.ForceRefTo(target)
        EndIf
    EndIf
    SetObjectiveDisplayed(aiRepairObjective, True, True)
EndFunction

Function FinishRepair(ReferenceAlias akRepairAlias, Int aiRepairObjective, Bool abClearAlias)
    SetObjectiveCompleted(aiRepairObjective)
    If abClearAlias && akRepairAlias != None
        akRepairAlias.Clear()
    EndIf
    FeedScript().ResumeFactory()
EndFunction

Function FailOpenObjectives()
    Int[] objectives = New Int[14]
    objectives[0] = 1
    objectives[1] = 5
    objectives[2] = 10
    objectives[3] = 20
    objectives[4] = 30
    objectives[5] = 50
    objectives[6] = 60
    objectives[7] = 62
    objectives[8] = 64
    objectives[9] = 66
    objectives[10] = 70
    objectives[11] = 75
    objectives[12] = 80
    objectives[13] = 85
    Int index = 0
    While index < objectives.Length
        Int objective = objectives[index]
        If IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
            SetObjectiveFailed(objective)
        EndIf
        index += 1
    EndWhile
EndFunction

Function CompleteDefenseObjectives()
    Int[] objectives = New Int[4]
    objectives[0] = 60
    objectives[1] = 62
    objectives[2] = 64
    objectives[3] = 66
    Int index = 0
    While index < objectives.Length
        If IsObjectiveDisplayed(objectives[index]) && !IsObjectiveFailed(objectives[index])
            SetObjectiveCompleted(objectives[index])
        EndIf
        index += 1
    EndWhile
EndFunction

Function Fragment_Stage_0010_Item_00()
    FeedScript().ResetFactory()
    FF06_Feed_Running.SetValue(1.0)
    SetMarkerEnabled(FF06_Feed_OverheatMarker, True)
    SetMarkerEnabled(FF06_Feed_OnMarker, False)
    ; The fuse box and valve only accept activation during their repair step; their aliases are filled then.
    Alias_RepairFuseBox.Clear()
    Alias_RepairValve.Clear()
    SetAliasRefEnabled(Alias_RadioBeacon, True)
    FeedScript().SetIngredientDispensersEnabled(False)
    SetObjectiveDisplayed(1, True, True)
    SetStage(20)
EndFunction

Function Fragment_Stage_0020_Item_00()
    ; The SpawnArea alias sets stage 30 when it loads; if it is already loaded that event will never come.
    ObjectReference spawnArea = Alias_SpawnArea.GetReference()
    If spawnArea != None && spawnArea.GetParentCell() != None && spawnArea.GetParentCell().IsAttached()
        SetStage(30)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    FeedScript().StartHarassment()
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(1)
    SetMarkerEnabled(FF06_Feed_OverheatMarker, False)
    SetMarkerEnabled(FF06_Feed_OnMarker, True)
    FeedScript().SpawnIngredients(Alias_IngredientsMeat, Alias_IngredientsStock, Alias_IngredientsVegetables)
    SetObjectiveDisplayed(5, True, True)
    SetObjectiveDisplayed(10, True, True)
    SetObjectiveDisplayed(20, True, True)
    SetObjectiveDisplayed(30, True, True)
    If !IsStageDone(30)
        SetStage(30)
    EndIf
    TryAdvanceToActivation()
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    TryAdvanceToActivation()
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20)
    TryAdvanceToActivation()
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    TryAdvanceToActivation()
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(5)
    SetObjectiveDisplayed(50, True, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60, True, True)
    SetObjectiveDisplayed(62, True, True)
    SetObjectiveDisplayed(64, True, True)
    SetObjectiveDisplayed(66, True, True)
    FeedScript().StartDefendPhase()
EndFunction

Function Fragment_Stage_0725_Item_00()
    FeedScript().BreakFactory()
EndFunction

Function Fragment_Stage_0728_Item_00()
    FeedScript().BreakFactory()
EndFunction

Function Fragment_Stage_0730_Item_00()
    ShowDiagnoseObjective()
EndFunction

Function Fragment_Stage_0735_Item_00()
    StartRepair(Alias_RepairFuseBox, 730, 75)
EndFunction

Function Fragment_Stage_0740_Item_00()
    FinishRepair(Alias_RepairFuseBox, 75, True)
EndFunction

Function Fragment_Stage_0745_Item_00()
    ShowDiagnoseObjective()
EndFunction

Function Fragment_Stage_0750_Item_00()
    StartRepair(Alias_RepairValve, 745, 80)
EndFunction

Function Fragment_Stage_0755_Item_00()
    FinishRepair(Alias_RepairValve, 80, True)
EndFunction

Function Fragment_Stage_0760_Item_00()
    ShowDiagnoseObjective()
EndFunction

Function Fragment_Stage_0765_Item_00()
    StartRepair(None, 760, 85)
    ; The pipe breaks when the diagnosis finds it, so the repair helper cannot run before this step.
    ObjectReference pipe = Alias_RepairPipe.GetReference()
    If pipe != None
        pipe.DamageObject(9999.0)
    EndIf
EndFunction

Function Fragment_Stage_0770_Item_00()
    FinishRepair(Alias_RepairPipe, 85, False)
EndFunction

Function Fragment_Stage_0800_Item_00()
    FeedScript().HandleMachineDestroyed(800)
EndFunction

Function Fragment_Stage_0850_Item_00()
    FeedScript().HandleMachineDestroyed(850)
EndFunction

Function Fragment_Stage_0900_Item_00()
    FeedScript().HandleMachineDestroyed(900)
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteDefenseObjectives()
    FeedScript().FinishEvent()
EndFunction

Function Fragment_Stage_1500_Item_00()
    FailOpenObjectives()
    FeedScript().FinishEvent()
EndFunction

Function Fragment_Stage_1550_Item_00()
    FeedScript().ShutdownFactory()
    SetMarkerEnabled(FF06_Feed_OnMarker, False)
    SetMarkerEnabled(FF06_Feed_OverheatMarker, True)
    SetAliasRefEnabled(Alias_RadioBeacon, False)
    Alias_RepairFuseBox.Clear()
    Alias_RepairValve.Clear()
    FF06_Feed_Running.SetValue(0.0)
EndFunction

Function Fragment_Stage_9991_Item_00()
    FailOpenObjectives()
    FeedScript().FinishEvent()
EndFunction

Function Fragment_Stage_10000_Item_00()
    FeedScript().DeleteUncollectedIngredients(Alias_IngredientsMeat)
    FeedScript().DeleteUncollectedIngredients(Alias_IngredientsStock)
    FeedScript().DeleteUncollectedIngredients(Alias_IngredientsVegetables)
    FF06_Feed_Running.SetValue(0.0)
EndFunction
