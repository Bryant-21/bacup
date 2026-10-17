; FO4 has no crafted-item event. A crafted item reaches the player with no source container,
; which separates it from looting; ground pickups also qualify and are accepted.
Event OnAliasInit()
    If ItemToCraft != None
        AddInventoryEventFilter(ItemToCraft)
    EndIf
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akBaseItem != ItemToCraft || akSourceContainer != None
        Return
    EndIf
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning() || owner.IsStageDone(StageToSet)
        Return
    EndIf
    If PrereqStage >= 0 && !owner.IsStageDone(PrereqStage)
        Return
    EndIf
    owner.SetStage(StageToSet)
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent
