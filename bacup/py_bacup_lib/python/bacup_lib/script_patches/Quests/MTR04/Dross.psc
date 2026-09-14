Event OnQuestInit()
	playerRef = QuestPlayer.GetActorReference()
	numberOfTargetsHit = 0
	numberOfDrossThrown = 0
	RemoveAllInventoryEventFilters()
	If playerRef != None
		playerRef.SetValue(TargetsHitRewardAV, 0.0)
		If DrossGrenade != None
			AddInventoryEventFilter(DrossGrenade)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnItemRemoved")
		RegisterForRemoteEvent(playerRef, "OnItemEquipped")
		RegisterForRemoteEvent(playerRef, "OnItemUnequipped")
	EndIf

	Int targetIndex = 0
	While targetIndex < TireTargets.Length
		ObjectReference targetRef = TireTargets[targetIndex].TargetTrigger.GetReference()
		If targetRef != None
			RegisterForRemoteEvent(targetRef, "OnTriggerEnter")
		EndIf
		targetIndex += 1
	EndWhile
EndEvent

Function BeginThrowing()
	numberOfTargetsHit = 0
	numberOfDrossThrown = 0
	If playerRef == None
		playerRef = QuestPlayer.GetActorReference()
	EndIf
	If playerRef != None
		playerRef.SetValue(TargetsHitRewardAV, 0.0)
		playerRef.AddItem(DrossGrenade, NumberOfChancesToThrow, True)
	EndIf
EndFunction

Function ScoreGame()
	If numberOfTargetsHit > 0
		SetStage(SuccessStage)
	Else
		SetStage(FailureStage)
	EndIf
EndFunction

Event ObjectReference.OnItemRemoved(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
	If akSender == playerRef && akBaseItem == DrossGrenade && IsStageDone(300) && !IsStageDone(RewardStage)
		numberOfDrossThrown += aiItemCount
		If numberOfDrossThrown >= NumberOfChancesToThrow
			StartTimer(SecondsToWaitOnLastThrow, lastDrossThrownTimerID)
		EndIf
	EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	If !IsStageDone(300) || IsStageDone(RewardStage) || akSender == None || akActionRef == None
		Return
	EndIf
	If akActionRef.GetDistance(akSender) > validDistanceFromDrossTarget
		Return
	EndIf

	Int targetIndex = 0
	While targetIndex < TireTargets.Length
		TargetData target = TireTargets[targetIndex]
		If target.TargetTrigger.GetReference() == akSender && !IsStageDone(target.StageToSetWhenHit)
			numberOfTargetsHit += 1
			If playerRef != None
				playerRef.SetValue(TargetsHitRewardAV, numberOfTargetsHit as Float)
			EndIf
			SetStage(target.StageToSetWhenHit)
			Return
		EndIf
		targetIndex += 1
	EndWhile
EndEvent

Event Actor.OnItemEquipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
	If akSender == playerRef && akBaseObject == Uniform && IsStageDone(201) && !IsStageDone(250)
		BoothWelcome.Start()
	EndIf
EndEvent

Event Actor.OnItemUnequipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
	If akSender == playerRef && akBaseObject == Uniform && IsStageDone(202) && !IsStageDone(250)
		BoothWelcome.Start()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == lastDrossThrownTimerID && !IsStageDone(RewardStage)
		SetStage(RewardStage)
	EndIf
EndEvent

Event OnQuestShutdown()
	UnregisterForAllEvents()
	RemoveAllInventoryEventFilters()
	If playerRef != None
		playerRef.RemoveItem(DrossGrenade, -1, True)
	EndIf

	mtr04_gamescomplete employeeTracker = EmployeeQuest as mtr04_gamescomplete
	If employeeTracker != None && EmployeeQuest.IsRunning() && EmployeeQuest.IsStageDone(800)
		employeeTracker.ActivityCompleted(0)
	EndIf
EndEvent
