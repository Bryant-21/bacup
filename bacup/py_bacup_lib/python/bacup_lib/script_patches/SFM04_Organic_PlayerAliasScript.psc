Event OnAliasInit()
    AddInventoryEventFilter(SFM04_Organic_RadShield)
    ReconcileRadShield()
EndEvent

Event OnPlayerLoadGame()
    ReconcileRadShield()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akBaseItem == SFM04_Organic_RadShield && aiItemCount > 0
        ReconcileRadShield()
    EndIf
EndEvent

Function ReconcileRadShield()
    Quest owningQuest = GetOwningQuest()
    Actor playerRef = GetActorReference()
    If owningQuest != None && playerRef != None && owningQuest.IsRunning() && owningQuest.IsStageDone(CraftRadshieldStage) && !owningQuest.IsStageDone(QuestCompleteStage) && playerRef.GetItemCount(SFM04_Organic_RadShield) > 0
        owningQuest.SetStage(QuestCompleteStage)
    EndIf
EndFunction
