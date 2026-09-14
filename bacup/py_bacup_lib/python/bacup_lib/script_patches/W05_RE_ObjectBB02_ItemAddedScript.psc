Event OnAliasInit()
    AddInventoryEventFilter(None)
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If PlayerAlias == None
        Return
    EndIf
    ObjectReference kPlayer = PlayerAlias.GetReference()
    Quest kQuest = GetOwningQuest()
    If kPlayer == None || kQuest == None || akSourceContainer != kPlayer || akBaseItem == None
        Return
    EndIf
    If akBaseItem.GetGoldValue() >= 20
        kQuest.SetStage(200)
    Else
        kQuest.SetStage(250)
    EndIf
EndEvent
