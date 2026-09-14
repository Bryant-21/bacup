Event OnQuestInit()
	EnsureLocalModuleLocation()
	InitializeLocalLockStages()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If ModuleExpeditionTeam_RefCollAlias != None && ModuleExpeditionTeam_RefCollAlias.Find(playerRef) < 0
			ModuleExpeditionTeam_RefCollAlias.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	ReconcileLocalLocks()
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
		ReconcileLocalLocks()
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	If akActivator == Game.GetPlayer()
		Utility.Wait(0.1)
		If !akSender.IsLocked()
			CompleteLocalLockForReference(akSender)
		EndIf
	EndIf
EndEvent

Event ObjectReference.OnLockStateChanged(ObjectReference akSender)
	If !akSender.IsLocked()
		CompleteLocalLockForReference(akSender)
	EndIf
EndEvent

Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
	If LockedObjectSets == None
		Return
	EndIf
	Int setIndex = 0
	While setIndex < LockedObjectSets.Length
		LockedObjectSetData setData = LockedObjectSets[setIndex]
		If setData.LockedObject_Form as Terminal == akSender && setData.LockedObject_Alias.GetReference() == akTerminalRef && setData.LockedTerminal_MenuItem == auiMenuItemID
			CompleteLocalLockSet(setIndex)
			Return
		EndIf
		setIndex += 1
	EndWhile
EndEvent

Function InitializeLocalLockStages()
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

Function ReconcileLocalLocks()
	If IsStageDone(CompleteStage) || LockedObjectSets == None
		Return
	EndIf
	LinearProg_i = 0
	Int setIndex = 0
	While setIndex < LockedObjectSets.Length
		LockedObjectSetData setData = LockedObjectSets[setIndex]
		If setData.StageToSetOnThisSetCompleted >= 0 && IsStageDone(setData.StageToSetOnThisSetCompleted)
			LinearProg_i += 1
		ElseIf AllowEarlyUnlocks || setIndex == LinearProg_i
			PrepareLocalLockSet(setIndex)
		EndIf
		setIndex += 1
	EndWhile
	If CountCompletedLocalLocks() >= CountValidLocalLocks() && CountValidLocalLocks() > 0
		SetStage(CompleteStage)
	EndIf
EndFunction

Function PrepareLocalLockSet(Int setIndex)
	If LockedObjectSets == None || setIndex < 0 || setIndex >= LockedObjectSets.Length
		Return
	EndIf
	LockedObjectSetData setData = LockedObjectSets[setIndex]
	ObjectReference lockedRef = setData.LockedObject_Alias.GetReference()
	Bool wasPrepared = setData.StageToSetOnThisSetPrepared >= 0 && IsStageDone(setData.StageToSetOnThisSetPrepared)
	If lockedRef == None && setData.LockedObject_Form != None && setData.LockedObject_PlacementPoint != None
		ObjectReference placementRef = setData.LockedObject_PlacementPoint.GetReference()
		If placementRef != None
			lockedRef = placementRef.PlaceAtMe(setData.LockedObject_Form, 1, True, False, False)
			If lockedRef != None
				lockedRef.MoveTo(placementRef, setData.PlacementOffset_X, setData.PlacementOffset_Y, setData.PlacementOffset_Z)
				setData.LockedObject_Alias.ForceRefTo(lockedRef)
			EndIf
		EndIf
	EndIf
	If lockedRef == None
		Return
	EndIf

	Terminal lockedTerminal = setData.LockedObject_Form as Terminal
	If lockedTerminal != None
		setData.LockedObject_Type = TYPE_TERMINAL
		RegisterForRemoteEvent(lockedTerminal, "OnMenuItemRun")
	Else
		setData.LockedObject_Type = TYPE_CONTAINER
		RegisterForRemoteEvent(lockedRef, "OnActivate")
		RegisterForRemoteEvent(lockedRef, "OnLockStateChanged")
		If !wasPrepared
			lockedRef.SetLockLevel(setData.LockedObject_LockLevel)
			lockedRef.Lock(True)
		EndIf
	EndIf
	If setData.StageToSetOnThisSetPrepared >= 0 && !wasPrepared
		SetStage(setData.StageToSetOnThisSetPrepared)
	EndIf
	If setData.StageToSetOnThisSetPrepared >= 0
		SetObjectiveDisplayed(setData.StageToSetOnThisSetPrepared, True)
	EndIf
EndFunction

Function CompleteLocalLockForReference(ObjectReference lockedRef)
	If LockedObjectSets == None || lockedRef == None
		Return
	EndIf
	Int setIndex = 0
	While setIndex < LockedObjectSets.Length
		If LockedObjectSets[setIndex].LockedObject_Alias.GetReference() == lockedRef
			CompleteLocalLockSet(setIndex)
			Return
		EndIf
		setIndex += 1
	EndWhile
EndFunction

Function CompleteLocalLockSet(Int setIndex)
	If IsStageDone(CompleteStage) || LockedObjectSets == None || setIndex < 0 || setIndex >= LockedObjectSets.Length
		Return
	EndIf
	LockedObjectSetData setData = LockedObjectSets[setIndex]
	If setData.StageToSetOnThisSetCompleted >= 0 && IsStageDone(setData.StageToSetOnThisSetCompleted)
		Return
	EndIf
	If setData.StageToSetOnThisSetCompleted >= 0
		SetStage(setData.StageToSetOnThisSetCompleted)
	EndIf
	If setData.StageToSetOnThisSetPrepared >= 0
		SetObjectiveCompleted(setData.StageToSetOnThisSetPrepared, True)
	EndIf
	If LockSolved_Message != None
		LockSolved_Message.Show()
	EndIf
	ReconcileLocalLocks()
EndFunction

Int Function CountCompletedLocalLocks()
	Int completedCount = 0
	Int setIndex = 0
	While LockedObjectSets != None && setIndex < LockedObjectSets.Length
		If LockedObjectSets[setIndex].StageToSetOnThisSetCompleted >= 0 && IsStageDone(LockedObjectSets[setIndex].StageToSetOnThisSetCompleted)
			completedCount += 1
		EndIf
		setIndex += 1
	EndWhile
	Return completedCount
EndFunction

Int Function CountValidLocalLocks()
	Int validCount = 0
	Int setIndex = 0
	While LockedObjectSets != None && setIndex < LockedObjectSets.Length
		LockedObjectSetData setData = LockedObjectSets[setIndex]
		If setData.LockedObject_Form != None && setData.LockedObject_Alias != None
			validCount += 1
		EndIf
		setIndex += 1
	EndWhile
	Return validCount
EndFunction
