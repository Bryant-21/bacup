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
    ; Deferred rather than run inline so the RunOnStart stage lands first -- sweeping
    ; during init could set a later stage before the quest's own start fragment ran.
    StartTimer(1.0, 9614)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 9614
        EvaluateCarriedItems(None)
    EndIf
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSender != Game.GetPlayer()
        Return
    EndIf
    EvaluateCarriedItems(akBaseItem)
EndEvent

; akAddedItem is the item that just arrived, or None for the once-per-run sweep. FO76
; only advanced the row whose item was added; the sweep has no such item, so it advances
; any row the player already satisfies. That is what makes a daily recoverable after an
; expired run left its quest item in the player's inventory: nothing would ever fire
; OnItemAdded for that item again, so the stage was unreachable on every later day and
; the symptom looked like a dead alias or dialogue rather than an inventory problem.
; Live carriers: Target Rich Environment 1155AF stage 300 (paper targets 13DB3D) and
; Buried with Honor 10C1E3 stage 300 (remains 12B666).
Function EvaluateCarriedItems(Form akAddedItem)
    If ItemStages == None
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
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
            currentCount = playerRef.GetItemCount(requiredItem)
        EndIf
        If requiredItem != None && (akAddedItem == None || requiredItem == akAddedItem) && currentCount >= requiredCount && ItemStages[index].stageToSet >= 0
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
EndFunction

Event OnQuestShutdown()
    CancelTimer(9614)
    UnregisterForRemoteEvent(Game.GetPlayer(), "OnItemAdded")
    RemoveAllInventoryEventFilters()
EndEvent
