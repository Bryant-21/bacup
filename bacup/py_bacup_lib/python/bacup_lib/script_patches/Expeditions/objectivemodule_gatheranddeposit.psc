Event OnQuestInit()
	EnsureLocalModuleLocation()
	InitializeLocalGatherStages()
	RebuildLocalItemFilters()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If ModuleExpeditionTeam_RefCollAlias != None && ModuleExpeditionTeam_RefCollAlias.Find(playerRef) < 0
			ModuleExpeditionTeam_RefCollAlias.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
		RegisterForRemoteEvent(playerRef, "OnItemAdded")
		RegisterForRemoteEvent(playerRef, "OnItemRemoved")
	EndIf
	PrepareLocalItemSets()
EndEvent

Function EnsureLocalModuleLocation()
	LocationAlias moduleLocation = GetAlias(3) as LocationAlias
	Actor playerRef = Game.GetPlayer()
	If moduleLocation != None && playerRef != None
		Location currentLocation = playerRef.GetCurrentLocation()
		If currentLocation != None && moduleLocation.GetLocation() != currentLocation
			moduleLocation.ForceLocationTo(currentLocation)
		EndIf
	EndIf
EndFunction

Event OnQuestShutdown()
	UnregisterForAllEvents()
	RemoveAllInventoryEventFilters()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer() && !IsStageDone(CompleteStage)
		RebuildLocalItemFilters()
		PrepareLocalItemSets()
	EndIf
EndEvent

Function RebuildLocalItemFilters()
	RemoveAllInventoryEventFilters()
	Int setIndex = 0
	While ItemsToDepositStructArray != None && setIndex < ItemsToDepositStructArray.Length
		Form itemFilter = ItemsToDepositStructArray[setIndex].ItemForm
		If itemFilter != None && !HasEarlierLocalItemFilter(itemFilter, setIndex)
			AddInventoryEventFilter(itemFilter)
		EndIf
		setIndex += 1
	EndWhile
EndFunction

Bool Function HasEarlierLocalItemFilter(Form itemFilter, Int beforeIndex)
	Int setIndex = 0
	While ItemsToDepositStructArray != None && setIndex < beforeIndex
		If ItemsToDepositStructArray[setIndex].ItemForm == itemFilter
			Return True
		EndIf
		setIndex += 1
	EndWhile
	Return False
EndFunction

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If akSender == Game.GetPlayer()
		ReconcileLocalItemCounts(akBaseItem)
	EndIf
EndEvent

Event ObjectReference.OnItemRemoved(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
	If akSender == Game.GetPlayer()
		ReconcileLocalItemCounts(akBaseItem)
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	If akActivator == Game.GetPlayer()
		DepositLocalItems(akSender)
	EndIf
EndEvent

Function InitializeLocalGatherStages()
	WaitingStage = XPD_ObjectiveModule_WaitingStage_Global.GetValue() as Int
	ActiveStage = XPD_ObjectiveModule_ActiveStage_Global.GetValue() as Int
	CompleteStage = XPD_ObjectiveModule_CompleteStage_Global.GetValue() as Int
	If !IsStageDone(WaitingStage)
		SetStage(WaitingStage)
	EndIf
	If !IsStageDone(ActiveStage)
		SetStage(ActiveStage)
	EndIf
	SetObjectiveCompleted(200, True)
	SetObjectiveDisplayed(300, True)
EndFunction

Function PrepareLocalItemSets()
	If IsStageDone(CompleteStage) || ItemsToDepositStructArray == None
		Return
	EndIf

	ItemSetsGathered = 0
	ItemDepositsCompleted = 0
	EWS_Script = (Self as Quest) as defaultquestencounterwavescript
	If EWS_Script != None && Overall_EWS_Wave >= 0
		EWS_Script.StartLocalEncounterWave(Overall_EWS_Wave)
	EndIf
	Int setIndex = 0
	While setIndex < ItemsToDepositStructArray.Length
		ItemToDepositStruct setData = ItemsToDepositStructArray[setIndex]
		If setData.ItemForm != None && setData.ItemsToGatherTotal > 0
			If setData.PlacedItemsToEnable != None
				setData.PlacedItemsToEnable.EnableAll()
			EndIf
			If setData.ContainersToPopulate != None
				setData.ContainersToPopulate.EnableAll()
			EndIf
			If setData.ActorsToPopulate != None
				setData.ActorsToPopulate.EnableAll()
			EndIf
			If EWS_Script != None && setData.EWS_Wave >= 0
				EWS_Script.StartLocalEncounterWave(setData.EWS_Wave)
			EndIf
			If setData.StageToSetOnItemsPrepared >= 0 && !IsStageDone(setData.StageToSetOnItemsPrepared)
				SetStage(setData.StageToSetOnItemsPrepared)
			EndIf
			If !setData.ItemsDeposited
				SetObjectiveDisplayed(setData.StageToSetOnItemsPrepared, True)
			EndIf
			ObjectReference depositPoint = setData.DepositPointAlias.GetReference()
			If depositPoint != None
				RegisterForRemoteEvent(depositPoint, "OnActivate")
			EndIf
			If setData.ItemsDeposited || IsStageDone(setData.StageToSetOnItemsDeposited)
				setData.ItemsDeposited = True
				ItemDepositsCompleted += 1
			EndIf
		EndIf
		setIndex += 1
	EndWhile
	ReconcileLocalItemCounts(None)
EndFunction

Function ReconcileLocalItemCounts(Form changedItem)
	If IsStageDone(CompleteStage) || ItemsToDepositStructArray == None
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf

	ItemSetsGathered = 0
	Int setIndex = 0
	While setIndex < ItemsToDepositStructArray.Length
		ItemToDepositStruct setData = ItemsToDepositStructArray[setIndex]
		If setData.ItemForm != None && setData.ItemsToGatherTotal > 0 && (changedItem == None || changedItem == setData.ItemForm)
			setData.ItemsGathered = playerRef.GetItemCount(setData.ItemForm)
			If setData.ItemsGathered > setData.ItemsToGatherTotal
				setData.ItemsGathered = setData.ItemsToGatherTotal
			EndIf
			If setData.ItemsGathered >= setData.ItemsToGatherTotal
				If !setData.AllItemsGathered
					setData.AllItemsGathered = True
					If setData.StageToSetOnItemsGathered >= 0 && !IsStageDone(setData.StageToSetOnItemsGathered)
						SetStage(setData.StageToSetOnItemsGathered)
					EndIf
				EndIf
				SetObjectiveCompleted(setData.StageToSetOnItemsPrepared, True)
				SetObjectiveDisplayed(setData.StageToSetOnItemsGathered, True)
			ElseIf !setData.ItemsDeposited
				setData.AllItemsGathered = False
			EndIf
		EndIf
		If setData.AllItemsGathered
			ItemSetsGathered += 1
		EndIf
		setIndex += 1
	EndWhile
EndFunction

Function DepositLocalItems(ObjectReference depositPoint)
	If IsStageDone(CompleteStage) || depositPoint == None || ItemsToDepositStructArray == None
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	Int setIndex = 0
	While setIndex < ItemsToDepositStructArray.Length
		ItemToDepositStruct setData = ItemsToDepositStructArray[setIndex]
		If !setData.ItemsDeposited && setData.DepositPointAlias.GetReference() == depositPoint && setData.ItemForm != None && setData.ItemsToGatherTotal > 0
			If playerRef.GetItemCount(setData.ItemForm) < setData.ItemsToGatherTotal
				Return
			EndIf
			setData.ItemsDeposited = True
			playerRef.RemoveItem(setData.ItemForm, setData.ItemsToGatherTotal, True, depositPoint)
			If setData.StageToSetOnItemsDeposited >= 0 && !IsStageDone(setData.StageToSetOnItemsDeposited)
				SetStage(setData.StageToSetOnItemsDeposited)
			EndIf
			SetObjectiveCompleted(setData.StageToSetOnItemsGathered, True)
			ItemDepositsCompleted += 1
			If setData.ItemDeposited_Message != None
				setData.ItemDeposited_Message.Show()
			EndIf
			If setData.ExplosionToPlay != None
				depositPoint.PlaceAtMe(setData.ExplosionToPlay)
			EndIf
			If setData.DisableDepositPointOnCompletion
				depositPoint.DisableNoWait()
			EndIf
			If ItemDepositsCompleted >= CountLocalItemSets()
				SetObjectiveCompleted(300, True)
				SetStage(CompleteStage)
			EndIf
			Return
		EndIf
		setIndex += 1
	EndWhile
EndFunction

Int Function CountLocalItemSets()
	Int itemSetCount = 0
	Int setIndex = 0
	While ItemsToDepositStructArray != None && setIndex < ItemsToDepositStructArray.Length
		ItemToDepositStruct setData = ItemsToDepositStructArray[setIndex]
		If setData.ItemForm != None && setData.ItemsToGatherTotal > 0
			itemSetCount += 1
		EndIf
		setIndex += 1
	EndWhile
	Return itemSetCount
EndFunction
