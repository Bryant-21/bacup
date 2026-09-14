Event OnAliasInit()
    PlayerRef = GetActorReference()
    BigFishQuestInstance = BigFishInASmallPond
    RemoveAllInventoryEventFilters()
    If isAFish != None
        AddInventoryEventFilter(isAFish)
    EndIf
    If PlayerRef != None
        chosenRegionInt = PlayerRef.GetValue(chosenRegionAV) as Int
        fishCaught = PlayerRef.GetValue(FishCaughtAV)
    EndIf
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If PlayerRef == None
        PlayerRef = GetActorReference()
    EndIf
    If BigFishQuestInstance == None
        BigFishQuestInstance = BigFishInASmallPond
    EndIf
    If PlayerRef == None || BigFishQuestInstance == None || akBaseItem == None || isAFish == None
        Return
    EndIf
    If BigFishQuestInstance.GetStage() < canFishNowStage || BigFishQuestInstance.IsStageDone(allFishCaughtStage)
        Return
    EndIf
    If !akBaseItem.HasKeyword(isAFish)
        Return
    EndIf

    chosenRegionInt = PlayerRef.GetValue(chosenRegionAV) as Int
    If chosenRegionInt < 0 || regionHierarchyKeywords == None || chosenRegionInt >= regionHierarchyKeywords.Length
        Return
    EndIf

    Location currentLocation = PlayerRef.GetCurrentLocation()
    If currentLocation == None || regionHierarchyKeywords[chosenRegionInt] == None || !currentLocation.HasKeyword(regionHierarchyKeywords[chosenRegionInt])
        Return
    EndIf

    fishCaught = PlayerRef.GetValue(FishCaughtAV) + aiItemCount as Float
    PlayerRef.SetValue(FishCaughtAV, fishCaught)
    If fishCaught >= totalRequiredFish
        BigFishQuestInstance.SetStage(allFishCaughtStage)
    EndIf
EndEvent
