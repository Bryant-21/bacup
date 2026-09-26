Event OnQuestInit()
	playerRef = QuestPlayer.GetActorReference()
	currentCartsActivated = 0
	PublishCartCounters()
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnItemEquipped")
		RegisterForRemoteEvent(playerRef, "OnItemUnequipped")
	EndIf
EndEvent

Function PublishCartCounters()
	Quest owner = Self as Quest
	B21:QuestVariables questVariables = owner as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable("Cartscount", currentCartsActivated as Float)
		Int cartCount = 0
		If Carts != None
			cartCount = Carts.Length
		EndIf
		questVariables.SetVariable("CartsRequired", cartCount as Float)
	EndIf
EndFunction

Function EvaluateBoothWelcome()
	If BoothWelcome != None
		BoothWelcome.Start()
	EndIf
	; Literal ids: the script may not declare new timer variables.
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

Function ScheduleSuccess()
	StartTimer(12.0, 94)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 92
		ResolveBoothWelcome()
	ElseIf aiTimerID == 94
		If IsStageDone(500) && !IsStageDone(1000)
			SetStage(1000)
		EndIf
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == cartsObjective
		currentCartsActivated = 0
		PublishCartCounters()
	ElseIf auiStageID >= 310 && auiStageID <= 340 && ((auiStageID - 310) % 10) == 0
		currentCartsActivated += 1
		PublishCartCounters()
		If PlaceCoalInCartSound != None && playerRef != None
			PlaceCoalInCartSound.Play(playerRef)
		EndIf
		If currentCartsActivated >= Carts.Length && !IsStageDone(completeStage)
			SetStage(completeStage)
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
	CancelTimer(92)
	CancelTimer(94)
	UnregisterForAllEvents()
	currentCartsActivated = 0
	PublishCartCounters()

	mtr04_gamescomplete employeeTracker = EmployeeQuest as mtr04_gamescomplete
	If employeeTracker != None && EmployeeQuest.IsRunning() && EmployeeQuest.IsStageDone(800)
		employeeTracker.ActivityCompleted(1)
	EndIf
EndEvent
