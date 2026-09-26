Event OnQuestInit()
	playerRef = QuestPlayer.GetActorReference()
	numOfHotdogsEaten = 0
	setPlayerEatHotdogLock = False
	PublishHotdogCounters()
	If playerRef != None
		playerRef.SetValue(MTR04_CanEatHotdog, 0.0)
		RegisterForRemoteEvent(playerRef, "OnItemEquipped")
		RegisterForRemoteEvent(playerRef, "OnItemUnequipped")
	EndIf
EndEvent

Function PublishHotdogCounters()
	Quest owner = Self as Quest
	B21:QuestVariables questVariables = owner as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable("HotdogsEaten", numOfHotdogsEaten as Float)
		questVariables.SetVariable("HotdogsRequired", RequiredHotdogsEatenCount as Float)
	EndIf
EndFunction

Function EvaluateBoothWelcome()
	If BoothWelcome != None
		BoothWelcome.Start()
	EndIf
	; 92/93 avoid digestingTimerID; the script may not declare new variables.
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

Function BeginEating()
	If playerRef == None
		playerRef = QuestPlayer.GetActorReference()
	EndIf
	numOfHotdogsEaten = 0
	setPlayerEatHotdogLock = False
	PublishHotdogCounters()
	If playerRef != None
		playerRef.SetValue(MTR04_CanEatHotdog, 1.0)
	EndIf

	If !ChowMasterQuest.IsRunning()
		ChowMasterQuest.Start()
	EndIf
	chowMasterQuestInstance = ChowMasterQuest
	Quests:MTR04:Chow_Master chowMaster = chowMasterQuestInstance as Quests:MTR04:Chow_Master
	If chowMaster != None
		chowMaster.PrepareHotdogs()
		Int hotdogIndex = 0
		While hotdogIndex < chowMaster.Hotdogs.GetCount()
			ObjectReference hotdogRef = chowMaster.Hotdogs.GetAt(hotdogIndex)
			If hotdogRef != None
				RegisterForRemoteEvent(hotdogRef, "OnActivate")
			EndIf
			hotdogIndex += 1
		EndWhile
	EndIf
EndFunction

Function FinishEating()
	setPlayerEatHotdogLock = True
	If playerRef != None
		playerRef.SetValue(MTR04_CanEatHotdog, 0.0)
	EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef != playerRef || setPlayerEatHotdogLock || !IsStageDone(hotdogMinigameStage) || IsStageDone(900) || IsStageDone(minigameCompleteStage)
		Return
	EndIf
	If playerRef.GetValue(MTR04_CanEatHotdog) < 1.0
		Return
	EndIf

	setPlayerEatHotdogLock = True
	playerRef.SetValue(MTR04_CanEatHotdog, 0.0)
	numOfHotdogsEaten += 1
	PublishHotdogCounters()
	If HotdogEatenSound != None
		HotdogEatenSound.Play(akSender)
	EndIf
	If HotdogEffects != None && HotdogEffects.Length > 0
		Int effectIndex = Utility.RandomInt(0, HotdogEffects.Length - 1)
		If HotdogEffects[effectIndex] != None
			HotdogEffects[effectIndex].Cast(playerRef, playerRef)
		EndIf
	EndIf

	If numOfHotdogsEaten >= RequiredHotdogsEatenCount
		SetStage(minigameCompleteStage)
	Else
		StartTimer(Utility.RandomFloat(DigestingTimeMin, DigestingTimeMax), digestingTimerID)
	EndIf

	Quests:MTR04:Chow_Master chowMaster = chowMasterQuestInstance as Quests:MTR04:Chow_Master
	If chowMaster != None
		chowMaster.RotateHotdog(akSender)
	Else
		akSender.DisableNoWait()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == digestingTimerID && !IsStageDone(900) && !IsStageDone(minigameCompleteStage)
		setPlayerEatHotdogLock = False
		If playerRef != None
			playerRef.SetValue(MTR04_CanEatHotdog, 1.0)
		EndIf
	ElseIf aiTimerID == 92
		ResolveBoothWelcome()
	ElseIf aiTimerID == 93
		If IsRunning()
			Stop()
		EndIf
	EndIf
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

Event OnQuestShutdown()
	FinishEating()
	CancelTimer(92)
	CancelTimer(93)
	UnregisterForAllEvents()
	If ChowMasterQuest != None && ChowMasterQuest.IsRunning()
		ChowMasterQuest.Stop()
	EndIf
	chowMasterQuestInstance = None
	numOfHotdogsEaten = 0
	PublishHotdogCounters()

	mtr04_gamescomplete employeeTracker = EmployeeQuest as mtr04_gamescomplete
	If employeeTracker != None && EmployeeQuest.IsRunning() && EmployeeQuest.IsStageDone(800)
		employeeTracker.ActivityCompleted(2)
	EndIf
EndEvent
