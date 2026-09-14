Event OnAliasInit()
    AddInventoryEventFilter(None)
EndEvent

Event OnItemRemoved(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    If PlayerAlias == None
        Return
    EndIf
    ObjectReference kPlayer = PlayerAlias.GetReference()
    Quest kQuest = GetOwningQuest()
    If kPlayer == None || kQuest == None || akDestContainer != kPlayer
        Return
    EndIf
    kQuest.SetStage(100)
EndEvent
