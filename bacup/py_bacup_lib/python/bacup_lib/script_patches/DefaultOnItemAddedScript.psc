Event OnQuestInit()
    If ItemStages != None
        Int index = 0
        While index < ItemStages.Length
            If ItemStages[index].itemFilter != None
                AddInventoryEventFilter(ItemStages[index].itemFilter)
            EndIf
            index += 1
        EndWhile
    EndIf
    RegisterForRemoteEvent(Game.GetPlayer(), "OnItemAdded")
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSender != Game.GetPlayer() || ItemStages == None
        Return
    EndIf

    Bool allItemsAdded = ItemStages.Length > 0
    Int index = 0
    While index < ItemStages.Length
        Form requiredItem = ItemStages[index].itemFilter
        Int requiredCount = ItemStages[index].count
        If requiredCount < 1
            requiredCount = 1
        EndIf

        Int currentCount = 0
        If requiredItem != None
            currentCount = akSender.GetItemCount(requiredItem)
        EndIf
        If requiredItem == akBaseItem && currentCount >= requiredCount && ItemStages[index].stageToSet >= 0
            SetStage(ItemStages[index].stageToSet)
        EndIf
        If requiredItem == None || currentCount < requiredCount
            allItemsAdded = False
        EndIf
        index += 1
    EndWhile

    If allItemsAdded && StageToSetWhenAllItemsAdded >= 0
        SetStage(StageToSetWhenAllItemsAdded)
    EndIf
EndEvent

Event OnQuestShutdown()
    UnregisterForRemoteEvent(Game.GetPlayer(), "OnItemAdded")
    RemoveAllInventoryEventFilters()
EndEvent
