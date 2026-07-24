; SHARD_PROTOCOL.md lesson #11: OnItemRemoved is opt-in and is never dispatched
; without a prior AddInventoryEventFilter registration on this script.
Event OnInit()
    AddInventoryEventFilter(None)
EndEvent

Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    If EggClusterMarkers.Length == 0
        Return
    EndIf

    If Utility.RandomFloat(0.0, 1.0) <= SpawnChance
        Int idx = Utility.RandomInt(0, EggClusterMarkers.Length - 1)
        EggClusterMarkers[idx].PlaceActorAtMe(EggDefender)
    EndIf
EndEvent
