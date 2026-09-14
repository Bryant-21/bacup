Event OnQuestInit()
	InitializeLocalSignalStages()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If ExpeditionTeam_RefCollAlias != None && ExpeditionTeam_RefCollAlias.Find(playerRef) < 0
			ExpeditionTeam_RefCollAlias.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	PrepareLocalSignalTargets()
EndEvent

Event OnQuestShutdown()
	UnregisterForAllEvents()
	ActiveSignalTargetSets.Clear()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer() && !IsStageDone(CompleteStage)
		If ActiveSignalTargetSets == None || ActiveSignalTargetSets.Length == 0
			PrepareLocalSignalTargets()
		Else
			RegisterLocalSignalTargetEvents()
		EndIf
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	If akActivator == Game.GetPlayer()
		CompleteLocalSignalTarget(akSender, TRACKED_DOWN_ACTIVATED)
	EndIf
EndEvent

Event ObjectReference.OnDestructionStageChanged(ObjectReference akSender, Int aiOldStage, Int aiCurrentStage)
	If akSender.IsDestroyed()
		CompleteLocalSignalTarget(akSender, TRACKED_DOWN_DESTROYED)
	EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
	CompleteLocalSignalTarget(akSender, TRACKED_DOWN_KILLED)
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If !UsingStageSetCriteria || SignalTargetSets == None
		Return
	EndIf
	Int setIndex = 0
	While setIndex < SignalTargetSets.Length
		SignalTargetData setData = SignalTargetSets[setIndex]
		If setData.TrackedDownCriteria == TRACKED_DOWN_STAGE_SET && setData.TrackedDownCriteriaStage == auiStageID
			CompleteLocalSignalTarget(setData.SignalTarget.GetReference(), TRACKED_DOWN_STAGE_SET)
		EndIf
		setIndex += 1
	EndWhile
EndEvent

Function InitializeLocalSignalStages()
	If !IsStageDone(WaitingStage)
		SetStage(WaitingStage)
	EndIf
	If !IsStageDone(ActiveStage)
		SetStage(ActiveStage)
	EndIf
	SetObjectiveDisplayed(ActiveStage + 10, True)
	SignalTransmitter = Transmitter_Alias.GetReference()
	If SignalTransmitter != None
		SignalTransmitter.EnableNoWait()
	EndIf
EndFunction

Function PrepareLocalSignalTargets()
	If IsStageDone(CompleteStage) || SignalTargetSets == None
		Return
	EndIf
	ActiveSignalTargetSets = new ObjectReference[0]
	UsingStageSetCriteria = False
	Int targetLimit = SignalTargetSets.Length
	If RandomizedSignalTargetsCount > 0 && RandomizedSignalTargetsCount < targetLimit
		targetLimit = RandomizedSignalTargetsCount
	EndIf
	Int setIndex = 0
	While setIndex < SignalTargetSets.Length && ActiveSignalTargetSets.Length < targetLimit
		SignalTargetData setData = SignalTargetSets[setIndex]
		ObjectReference targetRef = setData.SignalTarget.GetReference()
		Bool alreadyComplete = setData.TrackedDownDoneStage >= 0 && IsStageDone(setData.TrackedDownDoneStage)
		If targetRef != None && !alreadyComplete
			If setData.EnableOnStart || setData.EnableOnActive
				targetRef.EnableNoWait()
			EndIf
			If setData.SetUpDoneStage >= 0 && !IsStageDone(setData.SetUpDoneStage)
				SetStage(setData.SetUpDoneStage)
			EndIf
			ActiveSignalTargetSets.Add(targetRef)
			If setData.TrackedDownCriteria == TRACKED_DOWN_STAGE_SET
				UsingStageSetCriteria = True
			EndIf
		EndIf
		setIndex += 1
	EndWhile
	RegisterLocalSignalTargetEvents()
	ReconcileLocalSignalTargets()
EndFunction

Function RegisterLocalSignalTargetEvents()
	Int targetIndex = 0
	While ActiveSignalTargetSets != None && targetIndex < ActiveSignalTargetSets.Length
		ObjectReference targetRef = ActiveSignalTargetSets[targetIndex]
		Int setIndex = FindLocalSignalTargetSet(targetRef)
		If setIndex >= 0
			Int criteria = SignalTargetSets[setIndex].TrackedDownCriteria
			If criteria == TRACKED_DOWN_ACTIVATED
				RegisterForRemoteEvent(targetRef, "OnActivate")
			ElseIf criteria == TRACKED_DOWN_DESTROYED
				RegisterForRemoteEvent(targetRef, "OnDestructionStageChanged")
			ElseIf criteria == TRACKED_DOWN_KILLED
				RegisterForRemoteEvent(targetRef as Actor, "OnDeath")
			EndIf
		EndIf
		targetIndex += 1
	EndWhile
EndFunction

Function ReconcileLocalSignalTargets()
	Int targetIndex = 0
	While ActiveSignalTargetSets != None && targetIndex < ActiveSignalTargetSets.Length
		ObjectReference targetRef = ActiveSignalTargetSets[targetIndex]
		Int setIndex = FindLocalSignalTargetSet(targetRef)
		If setIndex < 0
			ActiveSignalTargetSets.Remove(targetIndex)
		Else
			SignalTargetData setData = SignalTargetSets[setIndex]
			Actor targetActor = targetRef as Actor
			If setData.TrackedDownCriteria == TRACKED_DOWN_DESTROYED && targetRef.IsDestroyed()
				CompleteLocalSignalTarget(targetRef, TRACKED_DOWN_DESTROYED)
			ElseIf setData.TrackedDownCriteria == TRACKED_DOWN_KILLED && targetActor != None && targetActor.IsDead()
				CompleteLocalSignalTarget(targetRef, TRACKED_DOWN_KILLED)
			ElseIf setData.TrackedDownCriteria == TRACKED_DOWN_STAGE_SET && setData.TrackedDownCriteriaStage >= 0 && IsStageDone(setData.TrackedDownCriteriaStage)
				CompleteLocalSignalTarget(targetRef, TRACKED_DOWN_STAGE_SET)
			Else
				targetIndex += 1
			EndIf
		EndIf
	EndWhile
	If ActiveSignalTargetSets != None && ActiveSignalTargetSets.Length == 0
		CompleteLocalSignalModule()
	EndIf
EndFunction

Function CompleteLocalSignalTarget(ObjectReference targetRef, Int criteria)
	If IsStageDone(CompleteStage) || targetRef == None || ActiveSignalTargetSets == None
		Return
	EndIf
	Int pendingIndex = ActiveSignalTargetSets.Find(targetRef)
	If pendingIndex < 0
		Return
	EndIf
	Int setIndex = FindLocalSignalTargetSet(targetRef)
	If setIndex < 0 || SignalTargetSets[setIndex].TrackedDownCriteria != criteria
		Return
	EndIf
	SignalTargetData setData = SignalTargetSets[setIndex]
	If setData.TrackedDownDoneStage >= 0 && !IsStageDone(setData.TrackedDownDoneStage)
		SetStage(setData.TrackedDownDoneStage)
	EndIf
	ActiveSignalTargetSets.Remove(pendingIndex)
	If ActiveSignalTargetSets.Length == 0
		CompleteLocalSignalModule()
	EndIf
EndFunction

Int Function FindLocalSignalTargetSet(ObjectReference targetRef)
	Int setIndex = 0
	While SignalTargetSets != None && setIndex < SignalTargetSets.Length
		If SignalTargetSets[setIndex].SignalTarget.GetReference() == targetRef
			Return setIndex
		EndIf
		setIndex += 1
	EndWhile
	Return -1
EndFunction

Function CompleteLocalSignalModule()
	If IsStageDone(CompleteStage)
		Return
	EndIf
	SetObjectiveCompleted(ActiveStage + 10, True)
	SetStage(CompleteStage)
EndFunction
