Event OnAliasInit()
    If FS01_MQ_Warn_BrokenUplinkMiscItem != None
        AddInventoryEventFilter(FS01_MQ_Warn_BrokenUplinkMiscItem)
    EndIf

    MTN_MQ_QuestScript controller = GetOwningQuest() as MTN_MQ_QuestScript
    If controller != None
        controller.MTNMQ_ReconcileProgress()
    EndIf
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    MTN_MQ_QuestScript controller = GetOwningQuest() as MTN_MQ_QuestScript
    If controller != None
        controller.MTNMQ_HandleDamagedUplinkAdded(akBaseItem, akItemReference)
    EndIf
EndEvent

Event OnPlayerLoadGame()
    MTN_MQ_QuestScript controller = GetOwningQuest() as MTN_MQ_QuestScript
    If controller != None
        controller.MTNMQ_ReconcileProgress()
    EndIf
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent
