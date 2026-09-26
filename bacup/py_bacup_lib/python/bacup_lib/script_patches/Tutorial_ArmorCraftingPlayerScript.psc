; The FO76 alias script is empty; the server reported the craft. In single player a
; armor that arrives from no container while a menu is open came from a workbench.
Event OnAliasInit()
    AddInventoryEventFilter(None)
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSourceContainer != None || !(akBaseItem is Armor) || !Utility.IsInMenuMode()
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest && owningQuest.IsStageDone(10) && !owningQuest.IsStageDone(20)
        owningQuest.SetStage(20)
    EndIf
EndEvent
