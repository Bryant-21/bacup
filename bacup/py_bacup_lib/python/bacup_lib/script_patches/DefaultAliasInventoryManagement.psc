Event OnAliasInit()
    ShutdownReferenceCache = GetReference()
    AddInventoryEventFilter(None)
    EvaluateInventoryState()
EndEvent

Event OnItemAdded(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    ObjectReference currentRef = GetReference()
    If currentRef != None
        ShutdownReferenceCache = currentRef
    EndIf
    If RemoveItemsOnAdded && !StopManagingInventoryFlag && IsManagedItem(akBaseItem) && currentRef != None
        CountRemovedButCounted += aiItemCount
        currentRef.RemoveItem(akBaseItem, aiItemCount, true)
    EndIf
    EvaluateInventoryState()
EndEvent

Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    ObjectReference currentRef = GetReference()
    If currentRef != None
        ShutdownReferenceCache = currentRef
    EndIf
    EvaluateInventoryState()
EndEvent

Event OnAliasShutdown()
    If RemoveItemsOnShutDown && !StopManagingInventoryFlag && ShutdownReferenceCache != None
        RemoveManagedItemsFrom(ShutdownReferenceCache)
    EndIf
    ShutdownReferenceCache = None
EndEvent

Function RemoveRequiredItems(bool OnlyIfHasAllItems = true, bool StopManagingInventory = true)
    ObjectReference watchedRef = GetReference()
    If watchedRef != None
        form[] itemsToRemove = RequiredItemsOverride
        If itemsToRemove == None
            itemsToRemove = RequiredItems
        EndIf
        If itemsToRemove != None && (!OnlyIfHasAllItems || HasAllManagedItems(watchedRef, itemsToRemove))
            RemoveManagedItemsFrom(watchedRef)
        EndIf
    EndIf
    If StopManagingInventory
        StopManagingInventoryFlag = true
    EndIf
EndFunction

Function TransferRequiredItemsFromPlayerToThisContainer(Actor PlayerToTakeItemsFrom)
    ObjectReference watchedRef = GetReference()
    If watchedRef == None || PlayerToTakeItemsFrom == None
        Return
    EndIf
    form[] itemsToTransfer = RequiredItemsOverride
    If itemsToTransfer == None
        itemsToTransfer = RequiredItems
    EndIf
    If itemsToTransfer == None
        Return
    EndIf
    Int i = 0
    While i < itemsToTransfer.Length
        Int have = PlayerToTakeItemsFrom.GetItemCount(itemsToTransfer[i])
        If have > 0
            PlayerToTakeItemsFrom.RemoveItem(itemsToTransfer[i], have, true, watchedRef)
        EndIf
        i += 1
    EndWhile
EndFunction

Function SetRequiredAmount(int amount)
    RequiredAmountOverride = amount
    EvaluateInventoryState()
EndFunction

Function SetRequiredItems(form[] requiredItems)
    RequiredItemsOverride = requiredItems
    EvaluateInventoryState()
EndFunction

Function EvaluateInventoryState()
    If StopManagingInventoryFlag
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None
        Return
    EndIf
    If RequireActivePlayerToComplete && Game.GetPlayer() == None
        Return
    EndIf

    If Objective > -1 && (StageToShowObjective == -1 || owningQuest.GetStage() >= StageToShowObjective)
        owningQuest.SetObjectiveDisplayed(Objective)
    EndIf

    Int threshold = RequiredAmount
    If RequiredAmountOverride > -1
        threshold = RequiredAmountOverride
    EndIf

    ; GetManagedCount() is called directly in each comparison rather than cached to
    ; a local: this compiler cannot type-check a same-script function's return value
    ; when it is assigned to a typed local (verified: bare comparisons/conditions are
    ; unaffected, only "Type x = SameScriptFn()" assignments are). Nothing between
    ; these calls mutates the watched container, so the repeated calls are safe.
    If GetManagedCount() >= threshold
        TryToSetStage()
        If AdditionalStageData != None
            Int i = 0
            While i < AdditionalStageData.Length
                If AdditionalStageData[i].Count <= GetManagedCount()
                    DefaultScriptFunctions.TryToSetStage(owningQuest, AdditionalStageData[i].StageToSet, PrereqStage, TurnOffStage)
                EndIf
                i += 1
            EndWhile
        EndIf
        If Objective > -1
            owningQuest.SetObjectiveCompleted(Objective)
        EndIf
        If NextObjectives != None
            Int j = 0
            While j < NextObjectives.Length
                owningQuest.SetObjectiveDisplayed(NextObjectives[j])
                j += 1
            EndWhile
        EndIf
    ElseIf DependentObjectives != None
        Int k = 0
        While k < DependentObjectives.Length
            owningQuest.SetObjectiveDisplayed(DependentObjectives[k], false)
            k += 1
        EndWhile
    EndIf
EndFunction

; RequiredItemsUseANDedKeywords is intentionally never read here -- see contract
; A.9.1: the one live carrier has no formlist-of-keywords entry to combine, and
; base FO4 Papyrus has no inventory-enumeration API to implement a true per-item
; keyword intersection even for a hypothetical future carrier.
Int Function GetManagedCount()
    ObjectReference watchedRef = GetReference()
    form[] itemsToCount = RequiredItemsOverride
    If itemsToCount == None
        itemsToCount = RequiredItems
    EndIf
    If watchedRef == None
        Return CountRemovedButCounted
    EndIf
    If itemsToCount == None
        Return watchedRef.GetItemCount(None) + CountRemovedButCounted
    EndIf
    Int total = CountRemovedButCounted
    Int i = 0
    While i < itemsToCount.Length
        total += watchedRef.GetItemCount(itemsToCount[i])
        i += 1
    EndWhile
    Return total
EndFunction

Bool Function IsManagedItem(Form akItem)
    form[] itemsToCheck = RequiredItemsOverride
    If itemsToCheck == None
        itemsToCheck = RequiredItems
    EndIf
    If itemsToCheck == None
        Return true
    EndIf
    Int i = 0
    While i < itemsToCheck.Length
        Form entry = itemsToCheck[i]
        If entry == akItem
            Return true
        ElseIf entry as Keyword != None && akItem.HasKeyword(entry as Keyword)
            Return true
        ElseIf entry as FormList != None && (entry as FormList).HasForm(akItem)
            Return true
        EndIf
        i += 1
    EndWhile
    Return false
EndFunction

Bool Function HasAllManagedItems(ObjectReference akContainer, form[] items)
    Int i = 0
    While i < items.Length
        If akContainer.GetItemCount(items[i]) <= 0
            Return false
        EndIf
        i += 1
    EndWhile
    Return true
EndFunction

Function RemoveManagedItemsFrom(ObjectReference akContainer)
    form[] itemsToRemove = RequiredItemsOverride
    If itemsToRemove == None
        itemsToRemove = RequiredItems
    EndIf
    If itemsToRemove == None
        Return
    EndIf
    Int i = 0
    While i < itemsToRemove.Length
        Int have = akContainer.GetItemCount(itemsToRemove[i])
        If have > 0
            akContainer.RemoveItem(itemsToRemove[i], have, true)
        EndIf
        i += 1
    EndWhile
EndFunction
