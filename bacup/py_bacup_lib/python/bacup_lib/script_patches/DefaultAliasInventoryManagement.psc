Event OnAliasInit()
    ; Daily quests stop and run again the next day on this same script instance, and
    ; Papyrus script variables survive a quest stop. Anything earned last run has to be
    ; cleared at both run boundaries or the next run starts already satisfied (or, with
    ; StopManagingInventoryFlag set, never evaluates at all).
    ClearRunState()
    ShutdownReferenceCache = GetReference()
    AddInventoryEventFilter(None)
    Quest hostQuest = GetOwningQuest()
    If hostQuest != None
        RegisterForRemoteEvent(hostQuest, "OnStageSet")
    EndIf
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    EvaluateInventoryState()
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == GetOwningQuest()
        EvaluateInventoryState()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        EvaluateInventoryState()
    EndIf
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
    UnregisterForRemoteEvent(GetOwningQuest(), "OnStageSet")
    UnregisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    RemoveAllInventoryEventFilters()
    If RemoveItemsOnShutDown && !StopManagingInventoryFlag && ShutdownReferenceCache != None
        RemoveManagedItemsFrom(ShutdownReferenceCache)
    EndIf
    ShutdownReferenceCache = None
    ClearRunState()
EndEvent

Function ClearRunState()
    StopManagingInventoryFlag = false
    CountRemovedButCounted = 0
    RequiredAmountOverride = -1
    RequiredItemsOverride = None
EndFunction

; The player alias this script watches can fill after OnAliasInit -- on Thrill of the
; Grill (23C7F3) alias 4 has no fill of its own and only DefaultQuestRemovePlayersScript
; force-fills it, one frame later -- so the inventory filter is re-armed whenever the
; watched reference first appears or changes.
Function RefreshWatchedReference()
    ObjectReference currentRef = GetReference()
    If currentRef != None && ShutdownReferenceCache != currentRef
        ShutdownReferenceCache = currentRef
        AddInventoryEventFilter(None)
    EndIf
EndFunction

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
    RefreshWatchedReference()
    Quest hostQuest = GetOwningQuest()
    If hostQuest == None || !hostQuest.IsRunning()
        Return
    EndIf
    If TurnOffStage >= 0 && hostQuest.GetStage() >= TurnOffStage
        Return
    EndIf
    If RequireActivePlayerToComplete && Game.GetPlayer() == None
        Return
    EndIf

    If Objective > -1 && (StageToShowObjective == -1 || hostQuest.GetStage() >= StageToShowObjective)
        hostQuest.SetObjectiveDisplayed(Objective)
    EndIf
    If PrereqStage >= 0 && !hostQuest.IsStageDone(PrereqStage)
        Return
    EndIf

    Int threshold = RequiredAmount
    If RequiredAmountOverride > -1
        threshold = RequiredAmountOverride
    EndIf

    ; Several rows on one quest can share a text variable (Mutual Aid 63D5BD writes
    ; "ItemCount" from four instances), and a row's prereq stage stays done once the
    ; quest moves on, so only the row whose objective is still open may publish.
    If Objective < 0 || !hostQuest.IsObjectiveCompleted(Objective)
        UpdateDisplayVariables(hostQuest, GetManagedCount(), threshold)
    EndIf

    ; GetManagedCount() is called directly in each comparison rather than cached to
    ; a local: this compiler cannot type-check a same-script function's return value
    ; when it is assigned to a typed local (verified: bare comparisons/conditions are
    ; unaffected, only "Type x = SameScriptFn()" assignments are). Nothing between
    ; these calls mutates the watched container, so the repeated calls are safe.
    If GetManagedCount() >= threshold
        ; This inlines DefaultAlias.TryToSetStage() (TurnOffWhenDead, then the global)
        ; because the wrapper returns nothing, and the old
        ; "!hostQuest.IsStageDone(StageToSet)" guard could not tell "prereq/turn-off
        ; blocked the stage" from "the conversion dropped that stage entirely". In the
        ; second case the stage can never become done, so the row returned here forever
        ; and neither Objective nor NextObjectives was ever reached -- Trick or Treat?
        ; 137931 alias 1 points at stage 400 on a quest whose stages are 1/100/200/300/
        ; 600/1000. The global's Bool reports the gating decision, which is what the
        ; guard actually meant.
        Actor aliasActor = GetActorRef()
        If TurnOffWhenDead && aliasActor != None && aliasActor.IsDead()
            Return
        EndIf
        If !DefaultScriptFunctions.TryToSetStage(hostQuest, StageToSet, PrereqStage, TurnOffStage)
            Return
        EndIf
        If AdditionalStageData != None
            Int i = 0
            While i < AdditionalStageData.Length
                If AdditionalStageData[i].Count <= GetManagedCount()
                    DefaultScriptFunctions.TryToSetStage(hostQuest, AdditionalStageData[i].StageToSet, PrereqStage, TurnOffStage)
                EndIf
                i += 1
            EndWhile
        EndIf
        If Objective > -1
            hostQuest.SetObjectiveCompleted(Objective)
        EndIf
        If NextObjectives != None
            Int j = 0
            While j < NextObjectives.Length
                hostQuest.SetObjectiveDisplayed(NextObjectives[j])
                j += 1
            EndWhile
        EndIf
    ElseIf DependentObjectives != None
        Int k = 0
        While k < DependentObjectives.Length
            hostQuest.SetObjectiveDisplayed(DependentObjectives[k], false)
            k += 1
        EndWhile
    EndIf
EndFunction

Function UpdateDisplayVariables(Quest akQuest, Int aiCount, Int aiThreshold)
    If akQuest == None || (ItemCountTextVar == "" && ItemRequiredAmountTextVar == "")
        Return
    EndIf
    B21:QuestVariables questVariables = akQuest as B21:QuestVariables
    If questVariables == None
        Return
    EndIf
    If ItemCountTextVar != ""
        Int shownCount = aiCount
        If shownCount > aiThreshold
            shownCount = aiThreshold
        EndIf
        questVariables.SetVariable(ItemCountTextVar, shownCount as Float)
    EndIf
    If ItemRequiredAmountTextVar != ""
        questVariables.SetVariable(ItemRequiredAmountTextVar, aiThreshold as Float)
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

; Bounded to what the quest actually counted, never the player's stack. RequiredItems on
; these rows are ordinary craftable flora -- A Refugee's Guide 63D33F sets
; RemoveItemsOnShutDown on all five of its rows (Bloodleaf, Cranberry, Blackberry,
; Aster, Silt Bean) -- and a player may be carrying hundreds harvested long before the
; daily was ever offered. Taking the whole stack on an abandoned run would be a
; player-hostile bug, and now that ExpireDailyQuest() stops an unfinished daily at the
; day boundary that path is routine rather than theoretical.
;
; The cap is a total across entries, mirroring GetManagedCount() which sums across them.
; Every live RemoveItemsOnShutDown carrier has a single RequiredItems entry, so the
; distribution across multiple entries is not exercised; a future multi-entry "one of
; each" turn-in would want a per-entry requirement, which FO76 does not record.
Function RemoveManagedItemsFrom(ObjectReference akContainer)
    form[] itemsToRemove = RequiredItemsOverride
    If itemsToRemove == None
        itemsToRemove = RequiredItems
    EndIf
    If itemsToRemove == None || akContainer == None
        Return
    EndIf

    Int remaining = RequiredAmount
    If RequiredAmountOverride > -1
        remaining = RequiredAmountOverride
    EndIf

    Int i = 0
    While i < itemsToRemove.Length && remaining > 0
        Int have = akContainer.GetItemCount(itemsToRemove[i])
        If have > remaining
            have = remaining
        EndIf
        If have > 0
            akContainer.RemoveItem(itemsToRemove[i], have, true)
            remaining -= have
        EndIf
        i += 1
    EndWhile
EndFunction
