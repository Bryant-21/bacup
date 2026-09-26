Event OnAliasInit()
    PlayerRef = GetActorReference()
    BigFishQuestInstance = BigFishInASmallPond
    RemoveAllInventoryEventFilters()
    If PlayerRef != None
        chosenRegionInt = PlayerRef.GetValue(chosenRegionAV) as Int
        fishCaught = PlayerRef.GetValue(FishCaughtAV)
    EndIf
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
EndEvent

Event OnPlayerLoadGame()
    RemoveAllInventoryEventFilters()
EndEvent

Function OnFishingCatch(Int aiItemCount, Int aiRegionMask)
    If PlayerRef == None
        PlayerRef = GetActorReference()
    EndIf
    If BigFishQuestInstance == None
        BigFishQuestInstance = BigFishInASmallPond
    EndIf
    If PlayerRef == None || BigFishQuestInstance == None || !BigFishQuestInstance.IsRunning() || aiItemCount <= 0
        Return
    EndIf
    If BigFishQuestInstance.GetStage() < canFishNowStage || BigFishQuestInstance.IsStageDone(allFishCaughtStage)
        Return
    EndIf
    chosenRegionInt = PlayerRef.GetValue(chosenRegionAV) as Int
    If chosenRegionInt < 0 || regionHierarchyKeywords == None || chosenRegionInt >= regionHierarchyKeywords.Length
        Return
    EndIf

    Int regionBit = 1
    Int index = 0
    While index < chosenRegionInt
        regionBit *= 2
        index += 1
    EndWhile
    If (aiRegionMask / regionBit) % 2 == 0
        Return
    EndIf

    fishCaught = PlayerRef.GetValue(FishCaughtAV) + aiItemCount as Float
    PlayerRef.SetValue(FishCaughtAV, fishCaught)
    If fishCaught >= totalRequiredFish
        BigFishQuestInstance.SetStage(allFishCaughtStage)
    EndIf
EndFunction
