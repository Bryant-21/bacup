Event OnAliasInit()
    RegisterRecipeGateFilters()
    ReconcileUnsupportedRecipeGate()
EndEvent

Event OnPlayerLoadGame()
    RemoveAllInventoryEventFilters()
    RegisterRecipeGateFilters()
    ReconcileUnsupportedRecipeGate()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Quest owningQuest = GetOwningQuest()
    If owningQuest && aiItemCount > 0
        If akBaseItem == ItemToCraft
            Bool prereqMet = PrereqStage < 0 || owningQuest.IsStageDone(PrereqStage)
            If prereqMet && owningQuest.GetStage() < StageToSet && !owningQuest.IsStageDone(StageToSet)
                owningQuest.SetStage(StageToSet)
            EndIf
        Else
            ReconcileUnsupportedRecipeGate()
        EndIf
    EndIf
EndEvent

Function ReconcileUnsupportedRecipeGate()
    Quest owningQuest = GetOwningQuest()
    ObjectReference playerRef = GetReference()
    If !owningQuest || !playerRef || !ItemToCraft || !owningQuest.IsRunning() || owningQuest.IsStageDone(StageToSet)
        Return
    EndIf
    If PrereqStage >= 0 && !owningQuest.IsStageDone(PrereqStage)
        Return
    EndIf
    If playerRef.GetItemCount(ItemToCraft) > 0
        owningQuest.SetStage(StageToSet)
        Return
    EndIf

    Form cookedRibeye = Game.GetFormFromFile(0x04695A, "SeventySix.esm")
    If ItemToCraft != cookedRibeye
        Return
    EndIf
    Form rawBrahminMeat = Game.GetFormFromFile(0x04A13F, "SeventySix.esm")
    Form wood = Game.GetFormFromFile(0x01FAC2, "Fallout4.esm")
    If rawBrahminMeat && wood && playerRef.GetItemCount(rawBrahminMeat) > 0 && playerRef.GetItemCount(wood) > 0
        playerRef.RemoveItem(rawBrahminMeat, 1, True)
        playerRef.RemoveItem(wood, 1, True)
        playerRef.AddItem(ItemToCraft, 1, False)
        If playerRef.GetItemCount(ItemToCraft) > 0 && !owningQuest.IsStageDone(StageToSet)
            owningQuest.SetStage(StageToSet)
        EndIf
    EndIf
EndFunction

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent

Function RegisterRecipeGateFilters()
    If ItemToCraft
        AddInventoryEventFilter(ItemToCraft)
    EndIf

    Form cookedRibeye = Game.GetFormFromFile(0x04695A, "SeventySix.esm")
    If ItemToCraft != cookedRibeye
        Return
    EndIf

    Form rawBrahminMeat = Game.GetFormFromFile(0x04A13F, "SeventySix.esm")
    Form wood = Game.GetFormFromFile(0x01FAC2, "Fallout4.esm")
    If rawBrahminMeat
        AddInventoryEventFilter(rawBrahminMeat)
    EndIf
    If wood
        AddInventoryEventFilter(wood)
    EndIf
EndFunction
