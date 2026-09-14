Event OnAliasInit()
    Actor playerRef = GetActorReference()
    Keyword objectTypeFish = Game.GetFormFromFile(0x007ABE8A, "SeventySix.esm") as Keyword
    RemoveAllInventoryEventFilters()
    If objectTypeFish != None
        AddInventoryEventFilter(objectTypeFish)
    EndIf
    If playerRef != None && FishCaughtAV != None
        playerRef.SetValue(FishCaughtAV, 0.0)
    EndIf
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Quest owningQuest = GetOwningQuest()
    Actor playerRef = GetActorReference()
    Keyword objectTypeFish = Game.GetFormFromFile(0x007ABE8A, "SeventySix.esm") as Keyword

    If owningQuest == None || playerRef == None || FishCaughtAV == None || objectTypeFish == None
        Return
    EndIf
    If !owningQuest.IsStageDone(500) || owningQuest.IsStageDone(600)
        Return
    EndIf
    If akBaseItem == None || !akBaseItem.HasKeyword(objectTypeFish)
        Return
    EndIf

    playerRef.ModValue(FishCaughtAV, aiItemCount as Float)
    If playerRef.GetValue(FishCaughtAV) >= 3.0
        owningQuest.SetStage(600)
    EndIf
EndEvent
