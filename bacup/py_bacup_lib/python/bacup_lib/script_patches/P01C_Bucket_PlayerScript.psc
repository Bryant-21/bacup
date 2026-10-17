; The repaired ProSnap camera (RepairedItem) reaching the player's inventory finishes the repair step (StageToSet).
Event OnAliasInit()
    AddInventoryEventFilter(RepairedItem)
    Actor player = GetActorReference()
    If player != None && player.GetItemCount(RepairedItem) > 0
        CameraRepaired(None)
    EndIf
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akBaseItem == RepairedItem
        CameraRepaired(akItemReference)
    EndIf
EndEvent

Function CameraRepaired(ObjectReference akCameraRef)
    Quest owner = GetOwningQuest()
    If owner == None || owner.IsStageDone(StageToSet)
        Return
    EndIf
    If akCameraRef != None && CameraAlias != None && CameraAlias.GetReference() == None
        CameraAlias.ForceRefTo(akCameraRef)
    EndIf
    owner.SetStage(StageToSet)
EndFunction
