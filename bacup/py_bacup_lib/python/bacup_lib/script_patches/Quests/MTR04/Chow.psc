Event OnQuestInit()
	playerRef = QuestPlayer.GetActorReference()
	numOfHotdogsEaten = 0
	setPlayerEatHotdogLock = False
	If playerRef != None
		playerRef.SetValue(MTR04_CanEatHotdog, 0.0)
		RegisterForRemoteEvent(playerRef, "OnItemEquipped")
		RegisterForRemoteEvent(playerRef, "OnItemUnequipped")
	EndIf
EndEvent

Function BeginEating()
	If playerRef == None
		playerRef = QuestPlayer.GetActorReference()
	EndIf
	numOfHotdogsEaten = 0
	setPlayerEatHotdogLock = False
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
	EndIf
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

Event OnQuestShutdown()
	FinishEating()
	UnregisterForAllEvents()
	mtr04_gamescomplete employeeTracker = EmployeeQuest as mtr04_gamescomplete
	If employeeTracker != None && EmployeeQuest.IsRunning() && EmployeeQuest.IsStageDone(800)
		employeeTracker.ActivityCompleted(2)
	EndIf
EndEvent
