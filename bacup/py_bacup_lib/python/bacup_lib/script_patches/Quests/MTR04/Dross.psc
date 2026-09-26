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

Function EvaluateBoothWelcome()
	If BoothWelcome != None
		BoothWelcome.Start()
	EndIf
	; 92/93 avoid lastDrossThrownTimerID; the script may not declare new variables.
	StartTimer(10.0, 92)
EndFunction

Function ResolveBoothWelcome()
	If !IsStageDone(200) || IsStageDone(250)
		Return
	EndIf
	If playerRef == None
		playerRef = QuestPlayer.GetActorReference()
	EndIf

	Bool employeeDone = EmployeeQuest != None && EmployeeQuest.IsCompleted()
	Bool wearingUniform = playerRef != None && Uniform != None && playerRef.IsEquipped(Uniform)
	If wearingUniform != employeeDone
		SetStage(250)
	ElseIf employeeDone
		If !IsStageDone(202)
			SetStage(202)
		EndIf
	Else
		If !IsStageDone(201)
			SetStage(201)
		EndIf
	EndIf
EndFunction

Function ScheduleShutdown()
	StartTimer(12.0, 93)
EndFunction

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
		EvaluateBoothWelcome()
	EndIf
EndEvent

Event Actor.OnItemUnequipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
	If akSender == playerRef && akBaseObject == Uniform && IsStageDone(202) && !IsStageDone(250)
		EvaluateBoothWelcome()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == lastDrossThrownTimerID && !IsStageDone(RewardStage)
		SetStage(RewardStage)
	ElseIf aiTimerID == 92
		ResolveBoothWelcome()
	ElseIf aiTimerID == 93
		If IsRunning()
			Stop()
		EndIf
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(92)
	CancelTimer(93)
	UnregisterForAllEvents()
	RemoveAllInventoryEventFilters()
	If playerRef != None
		playerRef.RemoveItem(DrossGrenade, -1, True)
		playerRef.SetValue(TargetsHitRewardAV, 0.0)
	EndIf
	numberOfTargetsHit = 0
	numberOfDrossThrown = 0

	mtr04_gamescomplete employeeTracker = EmployeeQuest as mtr04_gamescomplete
	If employeeTracker != None && EmployeeQuest.IsRunning() && EmployeeQuest.IsStageDone(800)
		employeeTracker.ActivityCompleted(0)
	EndIf
EndEvent
