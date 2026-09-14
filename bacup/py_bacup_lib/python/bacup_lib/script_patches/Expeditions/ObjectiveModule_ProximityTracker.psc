Event OnQuestInit()
	EnsureLocalModuleLocation()
	InitializeLocalTrackerStages()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If ModuleExpeditionTeam_RefCollAlias != None && ModuleExpeditionTeam_RefCollAlias.Find(playerRef) < 0
			ModuleExpeditionTeam_RefCollAlias.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	PrepareNextLocalSignalSet()
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
	AllPendingTargets.Clear()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer() && !IsStageDone(CompleteStage)
		If AllPendingTargets == None || AllPendingTargets.Length == 0
			PrepareNextLocalSignalSet()
		Else
			RegisterLocalSignalTargets()
		EndIf
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	If akActivator == Game.GetPlayer()
		HandleLocalSignalTarget(akSender)
	EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		HandleLocalSignalTarget(akSender)
	EndIf
EndEvent

Function InitializeLocalTrackerStages()
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
	SetObjectiveDisplayed(TrackerObjID, True)
	SignalTransmitter = Transmitter_Alias.GetReference()
	If SignalTransmitter != None
		SignalTransmitter.EnableNoWait()
	EndIf
	TargetSetsTrackedDown = 0
	TargetSetsTotal = 0
	If SignalTargetSets != None
		TargetSetsTotal = SignalTargetSets.Length
	EndIf
	If NumRandomTargetSets > 0 && NumRandomTargetSets < TargetSetsTotal
		TargetSetsTotal = NumRandomTargetSets
	EndIf
EndFunction

Function PrepareNextLocalSignalSet()
	If IsStageDone(CompleteStage) || SignalTargetSets == None
		Return
	EndIf
	While TargetSetsTrackedDown < TargetSetsTotal
		SignalTargetData setData = SignalTargetSets[TargetSetsTrackedDown]
		If setData.SignalTargets_RefColAlias != None && setData.SignalTargets_RefColAlias.GetCount() > 0
			If setData.StageToSetOnSetPrepared >= 0 && !IsStageDone(setData.StageToSetOnSetPrepared)
				SetStage(setData.StageToSetOnSetPrepared)
			EndIf
			If setData.StageToSetOnSetActive >= 0 && !IsStageDone(setData.StageToSetOnSetActive)
				SetStage(setData.StageToSetOnSetActive)
			EndIf
			If setData.StageToSetOnSetActive >= 0
				SetObjectiveDisplayed(setData.StageToSetOnSetActive, True)
			EndIf
			AllPendingTargets = new ObjectReference[0]
			Int targetLimit = setData.SignalTargets_RefColAlias.GetCount()
			If setData.NumRandomTargets > 0 && setData.NumRandomTargets < targetLimit
				targetLimit = setData.NumRandomTargets
			EndIf
			Int targetIndex = 0
			While targetIndex < targetLimit
				ObjectReference targetRef = setData.SignalTargets_RefColAlias.GetAt(targetIndex)
				If targetRef != None && targetIndex >= setData.SetTargetsTrackedDown
					targetRef.EnableNoWait()
					If setData.EnableLinkedRefOnSelection != None
						ObjectReference linkedRef = targetRef.GetLinkedRef(setData.EnableLinkedRefOnSelection)
						If linkedRef != None
							linkedRef.EnableNoWait()
						EndIf
					EndIf
					AllPendingTargets.Add(targetRef)
				EndIf
				targetIndex += 1
			EndWhile
			If AllPendingTargets.Length > 0
				RegisterLocalSignalTargets()
				Return
			EndIf
			CompleteCurrentLocalSignalSet()
		Else
			TargetSetsTrackedDown += 1
		EndIf
	EndWhile
	SetObjectiveCompleted(TrackerObjID, True)
	SetStage(CompleteStage)
EndFunction

Function RegisterLocalSignalTargets()
	Int targetIndex = 0
	While AllPendingTargets != None && targetIndex < AllPendingTargets.Length
		ObjectReference targetRef = AllPendingTargets[targetIndex]
		If targetRef != None
			RegisterForRemoteEvent(targetRef, "OnActivate")
			RegisterForRemoteEvent(targetRef, "OnTriggerEnter")
		EndIf
		targetIndex += 1
	EndWhile
EndFunction

Function HandleLocalSignalTarget(ObjectReference targetRef)
	If IsStageDone(CompleteStage) || AllPendingTargets == None
		Return
	EndIf
	Int pendingIndex = AllPendingTargets.Find(targetRef)
	If pendingIndex < 0
		Return
	EndIf

	AllPendingTargets.Remove(pendingIndex)
	SignalTargetData setData = SignalTargetSets[TargetSetsTrackedDown]
	setData.SetTargetsTrackedDown += 1
	If setData.PlayMessageOnActivation && XPD_ObjMod_ProximityTracker_ObjectTracked_Message != None
		XPD_ObjMod_ProximityTracker_ObjectTracked_Message.Show()
	EndIf
	If setData.ExplosionToPlaceOnActivation != None
		targetRef.PlaceAtMe(setData.ExplosionToPlaceOnActivation)
	EndIf
	If setData.DisableLinkedRefOnActivation != None
		ObjectReference linkedRef = targetRef.GetLinkedRef(setData.DisableLinkedRefOnActivation)
		If linkedRef != None
			linkedRef.DisableNoWait()
		EndIf
	EndIf
	If setData.DisableActivatorOnActivation
		targetRef.DisableNoWait()
	EndIf
	If AllPendingTargets.Length == 0
		CompleteCurrentLocalSignalSet()
		PrepareNextLocalSignalSet()
	EndIf
EndFunction

Function CompleteCurrentLocalSignalSet()
	If SignalTargetSets == None || TargetSetsTrackedDown < 0 || TargetSetsTrackedDown >= SignalTargetSets.Length
		Return
	EndIf
	SignalTargetData setData = SignalTargetSets[TargetSetsTrackedDown]
	If setData.SetTrackedDownDoneStage >= 0 && !IsStageDone(setData.SetTrackedDownDoneStage)
		SetStage(setData.SetTrackedDownDoneStage)
	EndIf
	If setData.StageToSetOnSetActive >= 0
		SetObjectiveCompleted(setData.StageToSetOnSetActive, True)
	EndIf
	TargetSetsTrackedDown += 1
EndFunction
