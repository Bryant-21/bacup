Function RegisterRemovalFilter()
    RemoveAllInventoryEventFilters()
    If ItemToFilter
        ObjectReference itemRef = ItemToFilter.GetReference()
        If itemRef
            AddInventoryEventFilter(itemRef.GetBaseObject())
        EndIf
    Else
        AddInventoryEventFilter(None)
    EndIf
EndFunction

Event OnAliasInit()
    RegisterRemovalFilter()
EndEvent

Event OnLoad()
    RegisterRemovalFilter()
EndEvent

Event OnItemRemoved(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Quest owner = GetOwningQuest()
    If !owner || !owner.IsRunning() || aiItemCount <= 0
        Return
    EndIf
    If ItemToFilter
        ObjectReference itemRef = ItemToFilter.GetReference()
        If !itemRef || (akItemReference != itemRef && akBaseItem != itemRef.GetBaseObject())
            Return
        EndIf
    EndIf
    TryToSetStage(PlayerRemoveType != 0, akDestContainer)
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent
