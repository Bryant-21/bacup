Event OnAliasInit()
    If MTNS01_RadioRepeater_Keyword != None
        AddInventoryEventFilter(MTNS01_RadioRepeater_Keyword)
    EndIf

    MTNS01QuestScript controller = GetOwningQuest() as MTNS01QuestScript
    If controller != None
        controller.MTNS01_ReconcileProgress()
    EndIf
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akBaseItem == None || MTNS01_RadioRepeater_Keyword == None \
        || !akBaseItem.HasKeyword(MTNS01_RadioRepeater_Keyword)
        Return
    EndIf

    If akItemReference != None && RadioRepeater != None
        RadioRepeater.ForceRefTo(akItemReference)
    EndIf

    MTNS01QuestScript controller = GetOwningQuest() as MTNS01QuestScript
    If controller != None
        controller.MTNS01_TrackRadioRepeater(akItemReference)
    EndIf
EndEvent

Event OnPlayerLoadGame()
    MTNS01QuestScript controller = GetOwningQuest() as MTNS01QuestScript
    If controller != None
        controller.MTNS01_ReconcileProgress()
    EndIf
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent
