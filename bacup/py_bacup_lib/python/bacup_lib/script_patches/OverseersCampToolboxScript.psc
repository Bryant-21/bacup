; OnItemRemoved is opt-in in FO4 (ScriptObject.AddInventoryEventFilter) and is never
; dispatched without it; None means "notify for any item". Registered in both OnInit
; (first-ever placement) and OnLoad (filters do not persist across save/load) so the
; event reaches this script on every subsequent visit, not just the first.
Event OnInit()
    AddInventoryEventFilter(None)
EndEvent

Event OnLoad()
    AddInventoryEventFilter(None)
EndEvent

Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    If akDestContainer == Game.GetPlayer() && Tutorial_Crafting_Materials_PlayerHasAcquired != None
        Game.GetPlayer().SetValue(Tutorial_Crafting_Materials_PlayerHasAcquired, 1.0)
    EndIf
EndEvent
