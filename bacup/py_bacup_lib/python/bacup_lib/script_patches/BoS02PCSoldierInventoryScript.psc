Event OnAliasInit()
    If pBoS02SoldierCertificate != None
        AddInventoryEventFilter(pBoS02SoldierCertificate)
    EndIf
    If pEN05_Basic != None
        RegisterForRemoteEvent(pEN05_Basic, "OnStageSet")
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None
        RegisterForRemoteEvent(owningQuest, "OnStageSet")
    EndIf
    ReconcileTrainingCompletion()
EndEvent

Function ReconcileTrainingCompletion()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning() || owningQuest.GetStage() < 400 || owningQuest.GetStage() >= 500
        Return
    EndIf
    Actor playerRef = GetActorReference()
    Bool hasCertificate = playerRef != None && pBoS02SoldierCertificate != None && playerRef.GetItemCount(pBoS02SoldierCertificate) > 0
    Bool trainingComplete = pEN05_Basic != None && (pEN05_Basic.IsCompleted() || pEN05_Basic.IsStageDone(150))
    If hasCertificate || trainingComplete
        owningQuest.SetStage(500)
    EndIf
EndFunction

Event OnPlayerLoadGame()
    OnAliasInit()
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    ReconcileTrainingCompletion()
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
    UnregisterForAllRemoteEvents()
EndEvent

Event OnItemAdded(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || owningQuest.GetStage() < 400 || owningQuest.GetStage() >= 500
        Return
    EndIf
    If akBaseItem == pBoS02SoldierCertificate && aiItemCount > 0
        ReconcileTrainingCompletion()
    EndIf
EndEvent
