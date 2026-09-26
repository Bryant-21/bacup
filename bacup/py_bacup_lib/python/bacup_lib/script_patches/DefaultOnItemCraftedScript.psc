Function RegisterCraftOutputFilters()
    RemoveAllInventoryEventFilters()
    Actor playerRef = Game.GetPlayer()
    If !playerRef || !EventCountData || !IsRunning()
        Return
    EndIf
    Int index = 0
    While index < EventCountData.Length
        If EventCountData[index].requiredForm
            AddInventoryEventFilter(EventCountData[index].requiredForm)
        EndIf
        index += 1
    EndWhile
    RegisterForRemoteEvent(playerRef, "OnItemAdded")
    RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
EndFunction

Function ReconcileCraftOutputs()
    Actor playerRef = Game.GetPlayer()
    If !StartCountingImmediately || !IsRunning() || IsCompleted() || !playerRef || !EventCountData
        Return
    EndIf
    Int index = 0
    While index < EventCountData.Length
        EventCountItem entry = EventCountData[index]
        If !entry.done && entry.requiredForm && entry.maxCount > 0 \
            && (entry.prereqStage < 0 || IsStageDone(entry.prereqStage))
            If entry.doneStage >= 0 && IsStageDone(entry.doneStage)
                entry.done = True
            Else
                entry.currentCount = playerRef.GetItemCount(entry.requiredForm)
                If entry.currentCount >= entry.maxCount
                    If entry.doneStage >= 0
                        entry.done = SetStage(entry.doneStage)
                    Else
                        entry.done = True
                    EndIf
                    If entry.done && entry.objectiveID >= 0
                        SetObjectiveCompleted(entry.objectiveID)
                    EndIf
                EndIf
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Event OnQuestInit()
    If EventCountData
        Int index = 0
        While index < EventCountData.Length
            EventCountData[index].done = False
            index += 1
        EndWhile
    EndIf
    RegisterCraftOutputFilters()
    ReconcileCraftOutputs()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    ReconcileCraftOutputs()
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSender == Game.GetPlayer() && aiItemCount > 0
        ReconcileCraftOutputs()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        RegisterCraftOutputFilters()
        ReconcileCraftOutputs()
    EndIf
EndEvent

Event OnQuestShutdown()
    RemoveAllInventoryEventFilters()
    Actor playerRef = Game.GetPlayer()
    If playerRef
        UnregisterForRemoteEvent(playerRef, "OnItemAdded")
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndEvent
