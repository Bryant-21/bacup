Event OnQuestInit()
	EnsureLocalModuleLocation()
	InitializeLocalDestructionStages()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	PrepareLocalDestructionSets()
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
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer() && !IsStageDone(CompleteStage)
		PrepareLocalDestructionSets()
	EndIf
EndEvent

Event ObjectReference.OnDestructionStageChanged(ObjectReference akSender, Int aiOldStage, Int aiCurrentStage)
	ReconcileLocalDestructionSets()
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	If akActivator == Game.GetPlayer()
		ReconcileLocalDestructionSets()
	EndIf
EndEvent

Function InitializeLocalDestructionStages()
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
EndFunction

Function PrepareLocalDestructionSets()
	If IsStageDone(CompleteStage) || DestructionSets == None
		Return
	EndIf
	Int setLimit = GetLocalDestructionSetLimit()
	Int setIndex = 0
	Int selectedSets = 0
	While setIndex < DestructionSets.Length && selectedSets < setLimit
		DestructionSetData setData = DestructionSets[setIndex]
		If setData.DestructibleObjects != None && setData.DestructibleObjects.GetCount() > 0
			selectedSets += 1
			Int objectLimit = setData.DestructibleObjects.GetCount()
			If setData.NumObjectsToEnable > 0 && setData.NumObjectsToEnable < objectLimit
				objectLimit = setData.NumObjectsToEnable
			EndIf
			setData.NumGoal = setData.NumObjectsToDestroy
			If setData.NumGoal <= 0 || setData.NumGoal > objectLimit
				setData.NumGoal = objectLimit
			EndIf
			Int objectIndex = 0
			While objectIndex < objectLimit
				ObjectReference destructibleRef = setData.DestructibleObjects.GetAt(objectIndex)
				If destructibleRef != None
					destructibleRef.EnableNoWait()
					RegisterForRemoteEvent(destructibleRef, "OnDestructionStageChanged")
					RegisterForRemoteEvent(destructibleRef, "OnActivate")
				EndIf
				objectIndex += 1
			EndWhile
			If setData.StageToSetOnThisSetActive >= 0 && !IsStageDone(setData.StageToSetOnThisSetActive)
				SetStage(setData.StageToSetOnThisSetActive)
			EndIf
			If setData.StageToSetOnThisSetActive >= 0
				SetObjectiveDisplayed(setData.StageToSetOnThisSetActive, True)
			EndIf
		EndIf
		setIndex += 1
	EndWhile
	ReconcileLocalDestructionSets()
EndFunction

Function ReconcileLocalDestructionSets()
	If IsStageDone(CompleteStage) || DestructionSets == None
		Return
	EndIf
	DestructionSetsComplete = 0
	Int setLimit = GetLocalDestructionSetLimit()
	Int setIndex = 0
	Int selectedSets = 0
	While setIndex < DestructionSets.Length && selectedSets < setLimit
		DestructionSetData setData = DestructionSets[setIndex]
		If setData.DestructibleObjects != None && setData.DestructibleObjects.GetCount() > 0
			selectedSets += 1
			setData.NumDestroyed = CountDestroyedLocalObjects(setData)
			Int remainingCount = setData.NumGoal - setData.NumDestroyed
			If remainingCount <= setData.ShowDirectQTsWhenXnumRemaining && setData.StageToSetOnXnumRemaining >= 0 && !IsStageDone(setData.StageToSetOnXnumRemaining)
				SetStage(setData.StageToSetOnXnumRemaining)
			EndIf
			If setData.NumGoal > 0 && setData.NumDestroyed >= setData.NumGoal
				If setData.StageToSetOnComplete >= 0 && !IsStageDone(setData.StageToSetOnComplete)
					SetStage(setData.StageToSetOnComplete)
				EndIf
				If setData.StageToSetOnThisSetActive >= 0
					SetObjectiveCompleted(setData.StageToSetOnThisSetActive, True)
				EndIf
				DestructionSetsComplete += 1
			EndIf
		EndIf
		setIndex += 1
	EndWhile
	If setLimit > 0 && DestructionSetsComplete >= setLimit
		SetStage(CompleteStage)
	EndIf
EndFunction

Int Function CountDestroyedLocalObjects(DestructionSetData setData)
	Int destroyedCount = 0
	Int objectLimit = setData.DestructibleObjects.GetCount()
	If setData.NumObjectsToEnable > 0 && setData.NumObjectsToEnable < objectLimit
		objectLimit = setData.NumObjectsToEnable
	EndIf
	Int objectIndex = 0
	While objectIndex < objectLimit
		ObjectReference destructibleRef = setData.DestructibleObjects.GetAt(objectIndex)
		If destructibleRef != None && (destructibleRef.IsDestroyed() || destructibleRef.IsDisabled())
			destroyedCount += 1
		EndIf
		objectIndex += 1
	EndWhile
	Return destroyedCount
EndFunction

Int Function GetLocalDestructionSetLimit()
	Int validCount = 0
	Int setIndex = 0
	While DestructionSets != None && setIndex < DestructionSets.Length
		DestructionSetData setData = DestructionSets[setIndex]
		If setData.DestructibleObjects != None && setData.DestructibleObjects.GetCount() > 0
			validCount += 1
		EndIf
		setIndex += 1
	EndWhile
	If NumSetsToDestroy > 0 && NumSetsToDestroy < validCount
		validCount = NumSetsToDestroy
	EndIf
	Return validCount
EndFunction
