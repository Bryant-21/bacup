Event OnQuestInit()
    currentStageIndexes = new Int[0]
    CurrentItemPropertiesArray = new ItemAndAmount[0]
    Int index = 0
    While SceneTriggersArray != None && index < SceneTriggersArray.Length
        If SceneTriggersArray[index] != None
            SceneTriggersArray[index].bDoneOnce = False
        EndIf
        index += 1
    EndWhile
    Actor playerRef = Game.GetPlayer()
    Bool filtered = False
    index = 0
    While ItemsPropertiesArray != None && index < ItemsPropertiesArray.Length
        If ItemsPropertiesArray[index] != None
            ItemsPropertiesArray[index].ItemAmount = 0
            If ItemsPropertiesArray[index].Item != None
                AddInventoryEventFilter(ItemsPropertiesArray[index].Item)
                filtered = True
            EndIf
        EndIf
        index += 1
    EndWhile
    If playerRef != None && filtered
        RegisterForRemoteEvent(playerRef, "OnItemAdded")
        RegisterForRemoteEvent(playerRef, "OnItemRemoved")
    EndIf
    RefreshStageWindows()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    RefreshStageWindows()
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    RecountItem(akBaseItem)
EndEvent

Event ObjectReference.OnItemRemoved(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    RecountItem(akBaseItem)
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
    If CurrentItemPropertiesArray == None
        CurrentItemPropertiesArray = new ItemAndAmount[0]
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
    While ItemsPropertiesArray != None && index < ItemsPropertiesArray.Length
        ItemAndAmount entry = ItemsPropertiesArray[index]
        If entry != None && entry.StagesIndex == aiStageIndex
            If CurrentItemPropertiesArray.Find(entry) < 0
                CurrentItemPropertiesArray.Add(entry)
            EndIf
            RecountEntry(entry)
        EndIf
        index += 1
    EndWhile
EndFunction

Function StopTrackingStage(Int aiStageIndex)
    Int index = CurrentItemPropertiesArray.Length - 1
    While index >= 0
        If CurrentItemPropertiesArray[index] == None || CurrentItemPropertiesArray[index].StagesIndex == aiStageIndex
            CurrentItemPropertiesArray.Remove(index)
        EndIf
        index -= 1
    EndWhile
EndFunction

; The objective shows what the player currently carries, so deposits and drops lower the count.
Function RecountItem(Form akBaseItem)
    Int index = 0
    While CurrentItemPropertiesArray != None && index < CurrentItemPropertiesArray.Length
        ItemAndAmount entry = CurrentItemPropertiesArray[index]
        If entry != None && entry.Item == akBaseItem
            RecountEntry(entry)
        EndIf
        index += 1
    EndWhile
EndFunction

Function RecountEntry(ItemAndAmount akEntry)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || akEntry.Item == None || currentStageIndexes.Find(akEntry.StagesIndex) < 0
        Return
    EndIf
    Int previous = akEntry.ItemAmount
    akEntry.ItemAmount = playerRef.GetItemCount(akEntry.Item)
    StagesStruct stage = StagesProperties[akEntry.StagesIndex]
    Int shown = akEntry.ItemAmount
    If shown > akEntry.ItemAmountToCollect && (stage == None || !stage.bAllowUIOverflow)
        shown = akEntry.ItemAmountToCollect
    EndIf
    SetQuestVariable(akEntry.TextReplacementString, shown)
    If akEntry.ItemAmount != previous
        CheckSceneTriggers(akEntry)
    EndIf
    CheckStageComplete(akEntry.StagesIndex)
EndFunction

Function CheckStageComplete(Int aiStageIndex)
    StagesStruct stage = StagesProperties[aiStageIndex]
    If stage == None || stage.StageToSet < 0 || IsStageDone(stage.StageToSet)
        Return
    EndIf
    Bool anyItem = False
    Int index = 0
    While ItemsPropertiesArray != None && index < ItemsPropertiesArray.Length
        ItemAndAmount entry = ItemsPropertiesArray[index]
        If entry != None && entry.StagesIndex == aiStageIndex
            anyItem = True
            If entry.ItemAmount < entry.ItemAmountToCollect
                Return
            EndIf
        EndIf
        index += 1
    EndWhile
    If anyItem
        SetStage(stage.StageToSet)
    EndIf
EndFunction

Function CheckSceneTriggers(ItemAndAmount akChanged)
    Int index = 0
    While SceneTriggersArray != None && index < SceneTriggersArray.Length
        SceneTriggerStruct sceneTrigger = SceneTriggersArray[index]
        If sceneTrigger != None && sceneTrigger.SceneToTrigger != None && !(sceneTrigger.bDoOnce && sceneTrigger.bDoneOnce)
            Bool related = False
            If sceneTrigger.TextReplacementStringRelatedTo != ""
                related = sceneTrigger.TextReplacementStringRelatedTo == akChanged.TextReplacementString
            Else
                related = sceneTrigger.ItemRelatedTo != None && sceneTrigger.ItemRelatedTo == akChanged.Item
            EndIf
            If related && CompareCount(akChanged.ItemAmount, sceneTrigger.comparingOperator, sceneTrigger.CountToTriggerAt)
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
