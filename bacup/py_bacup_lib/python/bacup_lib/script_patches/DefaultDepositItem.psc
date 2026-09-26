Event OnQuestInit()
    B21DepositBusy = False
    currentStageIndexes = new Int[0]
    CurrentDepositPropertiesArray = new DepositProperties[0]
    CurrentDepositActivatorsArray = new DepositActivatorsStruct[0]
    Int index = 0
    While DepositPropertiesArray != None && index < DepositPropertiesArray.Length
        If DepositPropertiesArray[index] != None
            DepositPropertiesArray[index].DepositedAmount = 0
        EndIf
        index += 1
    EndWhile
    index = 0
    While SceneTriggersArray != None && index < SceneTriggersArray.Length
        If SceneTriggersArray[index] != None
            SceneTriggersArray[index].bDoneOnce = False
        EndIf
        index += 1
    EndWhile
    index = 0
    While DepositActivatorsArray != None && index < DepositActivatorsArray.Length
        If DepositActivatorsArray[index] != None && DepositActivatorsArray[index].DepositActivator != None
            RegisterForRemoteEvent(DepositActivatorsArray[index].DepositActivator, "OnActivate")
        EndIf
        index += 1
    EndWhile
    RefreshStageWindows()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    RefreshStageWindows()
EndEvent

Event OnQuestShutdown()
    Actor playerRef = Game.GetPlayer()
    Int index = 0
    While playerRef != None && DepositPropertiesArray != None && index < DepositPropertiesArray.Length
        DepositProperties deposit = DepositPropertiesArray[index]
        If deposit != None && deposit.bCleanUp && deposit.ItemToDeposit != None && playerRef.GetItemCount(deposit.ItemToDeposit) > 0
            playerRef.RemoveItem(deposit.ItemToDeposit, playerRef.GetItemCount(deposit.ItemToDeposit), True)
        EndIf
        index += 1
    EndWhile
EndEvent

Event ReferenceAlias.OnActivate(ReferenceAlias akSender, ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef != playerRef || B21DepositBusy
        Return
    EndIf
    B21DepositBusy = True
    Int index = 0
    While DepositActivatorsArray != None && index < DepositActivatorsArray.Length
        DepositActivatorsStruct depositPoint = DepositActivatorsArray[index]
        If depositPoint != None && depositPoint.DepositActivator == akSender && currentStageIndexes.Find(depositPoint.StagesIndex) >= 0
            DepositForStage(depositPoint.StagesIndex, playerRef)
        EndIf
        index += 1
    EndWhile
    B21DepositBusy = False
EndEvent

Bool Function StageWindowOpen(Int aiStageIndex)
    StagesStruct stage = StagesProperties[aiStageIndex]
    If stage == None || !IsStageDone(stage.preReqStage)
        Return False
    EndIf
    If stage.EndStage > 0 && IsStageDone(stage.EndStage)
        Return False
    EndIf
    Return stage.StageToSet < 0 || !IsStageDone(stage.StageToSet)
EndFunction

Function RefreshStageWindows()
    If StagesProperties == None
        Return
    EndIf
    If currentStageIndexes == None
        currentStageIndexes = new Int[0]
    EndIf
    If CurrentDepositPropertiesArray == None
        CurrentDepositPropertiesArray = new DepositProperties[0]
    EndIf
    If CurrentDepositActivatorsArray == None
        CurrentDepositActivatorsArray = new DepositActivatorsStruct[0]
    EndIf
    Int index = 0
    While index < StagesProperties.Length
        Bool open = StageWindowOpen(index)
        Int slot = currentStageIndexes.Find(index)
        If open && slot < 0
            currentStageIndexes.Add(index)
            StartTrackingStage(index)
        ElseIf !open && slot >= 0
            currentStageIndexes.Remove(slot)
            StopTrackingStage(index)
        EndIf
        index += 1
    EndWhile
EndFunction

Function StartTrackingStage(Int aiStageIndex)
    Int index = 0
    While DepositPropertiesArray != None && index < DepositPropertiesArray.Length
        DepositProperties deposit = DepositPropertiesArray[index]
        If deposit != None && deposit.StagesIndex == aiStageIndex
            If CurrentDepositPropertiesArray.Find(deposit) < 0
                CurrentDepositPropertiesArray.Add(deposit)
            EndIf
            UpdateDepositText(deposit, StagesProperties[aiStageIndex])
        EndIf
        index += 1
    EndWhile
    index = 0
    While DepositActivatorsArray != None && index < DepositActivatorsArray.Length
        DepositActivatorsStruct depositPoint = DepositActivatorsArray[index]
        If depositPoint != None && depositPoint.StagesIndex == aiStageIndex && CurrentDepositActivatorsArray.Find(depositPoint) < 0
            CurrentDepositActivatorsArray.Add(depositPoint)
        EndIf
        index += 1
    EndWhile
EndFunction

Function StopTrackingStage(Int aiStageIndex)
    Int index = CurrentDepositPropertiesArray.Length - 1
    While index >= 0
        If CurrentDepositPropertiesArray[index] == None || CurrentDepositPropertiesArray[index].StagesIndex == aiStageIndex
            CurrentDepositPropertiesArray.Remove(index)
        EndIf
        index -= 1
    EndWhile
    index = CurrentDepositActivatorsArray.Length - 1
    While index >= 0
        If CurrentDepositActivatorsArray[index] == None || CurrentDepositActivatorsArray[index].StagesIndex == aiStageIndex
            CurrentDepositActivatorsArray.Remove(index)
        EndIf
        index -= 1
    EndWhile
EndFunction

Function DepositForStage(Int aiStageIndex, Actor akPlayer)
    StagesStruct stage = StagesProperties[aiStageIndex]
    Int index = 0
    While DepositPropertiesArray != None && index < DepositPropertiesArray.Length
        DepositProperties deposit = DepositPropertiesArray[index]
        If deposit != None && deposit.StagesIndex == aiStageIndex && deposit.ItemToDeposit != None
            Int held = akPlayer.GetItemCount(deposit.ItemToDeposit)
            Int amount = held
            If !deposit.bDepositAll && amount > deposit.ItemAmountToDeposit - deposit.DepositedAmount
                amount = deposit.ItemAmountToDeposit - deposit.DepositedAmount
            EndIf
            If amount > 0
                akPlayer.RemoveItem(deposit.ItemToDeposit, amount)
                deposit.DepositedAmount += held - akPlayer.GetItemCount(deposit.ItemToDeposit)
                UpdateDepositText(deposit, stage)
                CheckSceneTriggers(deposit)
            EndIf
        EndIf
        index += 1
    EndWhile
    CheckStageComplete(aiStageIndex)
EndFunction

Function UpdateDepositText(DepositProperties akDeposit, StagesStruct akStage)
    Int shown = akDeposit.DepositedAmount
    If shown > akDeposit.ItemAmountToDeposit && (akStage == None || !akStage.bAllowUIOverflow)
        shown = akDeposit.ItemAmountToDeposit
    EndIf
    SetQuestVariable(akDeposit.TextReplacementString, shown)
EndFunction

Function CheckStageComplete(Int aiStageIndex)
    StagesStruct stage = StagesProperties[aiStageIndex]
    If stage == None || stage.StageToSet < 0 || IsStageDone(stage.StageToSet)
        Return
    EndIf
    Bool anyDeposit = False
    Int index = 0
    While DepositPropertiesArray != None && index < DepositPropertiesArray.Length
        DepositProperties deposit = DepositPropertiesArray[index]
        If deposit != None && deposit.StagesIndex == aiStageIndex
            anyDeposit = True
            If deposit.DepositedAmount < deposit.ItemAmountToDeposit
                Return
            EndIf
        EndIf
        index += 1
    EndWhile
    If anyDeposit
        SetStage(stage.StageToSet)
    EndIf
EndFunction

Function CheckSceneTriggers(DepositProperties akChanged)
    Int index = 0
    While SceneTriggersArray != None && index < SceneTriggersArray.Length
        SceneTriggerStruct sceneTrigger = SceneTriggersArray[index]
        If sceneTrigger != None && sceneTrigger.SceneToTrigger != None && !(sceneTrigger.bDoOnce && sceneTrigger.bDoneOnce)
            Bool related = False
            If sceneTrigger.TextReplacementStringRelatedTo != ""
                related = sceneTrigger.TextReplacementStringRelatedTo == akChanged.TextReplacementString
            Else
                related = sceneTrigger.ItemRelatedTo != None && sceneTrigger.ItemRelatedTo == akChanged.ItemToDeposit
            EndIf
            If related && CompareCount(akChanged.DepositedAmount, sceneTrigger.comparingOperator, sceneTrigger.CountToTriggerAt)
                sceneTrigger.bDoneOnce = True
                If sceneTrigger.bForceScene
                    sceneTrigger.SceneToTrigger.ForceStart()
                Else
                    sceneTrigger.SceneToTrigger.Start()
                EndIf
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Bool Function CompareCount(Int aiValue, String asOperator, Int aiTarget)
    If asOperator == ">"
        Return aiValue > aiTarget
    ElseIf asOperator == "<"
        Return aiValue < aiTarget
    ElseIf asOperator == "<=" || asOperator == "=<"
        Return aiValue <= aiTarget
    ElseIf asOperator == "==" || asOperator == "="
        Return aiValue == aiTarget
    ElseIf asOperator == "!="
        Return aiValue != aiTarget
    EndIf
    Return aiValue >= aiTarget
EndFunction

Function SetQuestVariable(String asName, Int aiValue)
    If asName == ""
        Return
    EndIf
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables != None
        variables.SetVariable(asName, aiValue as Float)
    EndIf
EndFunction
