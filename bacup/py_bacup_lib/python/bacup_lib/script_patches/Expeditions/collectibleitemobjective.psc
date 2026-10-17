Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == Stage_SetUpObjective
		SetUpCollectibleObjective()
	ElseIf auiStageID == Stage_CollectedAllItems
		DisarmCollectibleListener()
	EndIf
EndEvent

Event OnQuestShutdown()
	DisarmCollectibleListener()
EndEvent

Function SetUpCollectibleObjective()
	If CollectibleItem == None || IsStageDone(Stage_CollectedAllItems)
		Return
	EndIf
	Int placedCount = EnableCollectibleRefs(PlacedItemAlias_Exterior) + EnableCollectibleRefs(PlacedItemAlias_Interior)
	If placedCount < ItemsToCollectTotal
		placedCount += StockCollectibleContainers(ContainerAlias_Exterior, ItemsToCollectTotal - placedCount)
	EndIf
	If placedCount < ItemsToCollectTotal
		StockCollectibleContainers(ContainerAlias_Interior, ItemsToCollectTotal - placedCount)
	EndIf
	B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
	If questVariables != None && TextVar_ItemsToCollectTotal != ""
		questVariables.SetVariable(TextVar_ItemsToCollectTotal, ItemsToCollectTotal as Float)
	EndIf
	Actor playerRef = Game.GetPlayer()
	AddInventoryEventFilter(CollectibleItem)
	RegisterForRemoteEvent(playerRef, "OnItemAdded")
	RegisterForRemoteEvent(playerRef, "OnItemRemoved")
	UpdateCollectedItems()
EndFunction

Int Function EnableCollectibleRefs(RefCollectionAlias akItems)
	If akItems == None
		Return 0
	EndIf
	akItems.EnableAll()
	Return akItems.GetCount()
EndFunction

; Containers are walked from a random offset so a single-item hunt (AC01's one stolen
; winnings among the cheater corpses) does not always land in the same corpse.
Int Function StockCollectibleContainers(RefCollectionAlias akContainers, Int aiNeeded)
	If akContainers == None || aiNeeded <= 0
		Return 0
	EndIf
	Int count = akContainers.GetCount()
	If count <= 0
		Return 0
	EndIf
	Int offset = Utility.RandomInt(0, count - 1)
	Int stocked = 0
	Int index = 0
	While index < count && stocked < aiNeeded
		ObjectReference containerRef = akContainers.GetAt((index + offset) % count)
		If containerRef != None && containerRef.GetItemCount(CollectibleItem) <= 0
			containerRef.AddItem(CollectibleItem, 1, True)
			stocked += 1
		EndIf
		index += 1
	EndWhile
	Return stocked
EndFunction

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If akSender == Game.GetPlayer()
		UpdateCollectedItems()
	EndIf
EndEvent

Event ObjectReference.OnItemRemoved(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
	If akSender == Game.GetPlayer()
		UpdateCollectedItems()
	EndIf
EndEvent

Function UpdateCollectedItems()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || CollectibleItem == None || IsStageDone(Stage_CollectedAllItems)
		Return
	EndIf
	ItemsCollected = playerRef.GetItemCount(CollectibleItem)
	If ItemsCollected > ItemsToCollectTotal
		ItemsCollected = ItemsToCollectTotal
	EndIf
	B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
	If questVariables != None && TextVar_ItemsCollected != ""
		questVariables.SetVariable(TextVar_ItemsCollected, ItemsCollected as Float)
	EndIf
	If ItemsCollected_AV != None
		playerRef.SetValue(ItemsCollected_AV, ItemsCollected as Float)
	EndIf
	If ItemsCollected >= ItemsToCollectTotal
		SetStage(Stage_CollectedAllItems)
	EndIf
EndFunction

Function DisarmCollectibleListener()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnItemAdded")
		UnregisterForRemoteEvent(playerRef, "OnItemRemoved")
	EndIf
	RemoveAllInventoryEventFilters()
EndFunction
