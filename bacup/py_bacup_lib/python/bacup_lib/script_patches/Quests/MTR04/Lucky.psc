Event OnQuestInit()
	playerRef = QuestPlayer.GetActorReference()
	currentCartsActivated = 0
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnItemEquipped")
		RegisterForRemoteEvent(playerRef, "OnItemUnequipped")
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == cartsObjective
		currentCartsActivated = 0
	ElseIf auiStageID >= 310 && auiStageID <= 340 && ((auiStageID - 310) % 10) == 0
		currentCartsActivated += 1
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
		BoothWelcome.Start()
	EndIf
EndEvent

Event Actor.OnItemUnequipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
	If akSender == playerRef && akBaseObject == Uniform && IsStageDone(202) && !IsStageDone(250)
		BoothWelcome.Start()
	EndIf
EndEvent

Event OnQuestShutdown()
	UnregisterForAllEvents()
	mtr04_gamescomplete employeeTracker = EmployeeQuest as mtr04_gamescomplete
	If employeeTracker != None && EmployeeQuest.IsRunning() && EmployeeQuest.IsStageDone(800)
		employeeTracker.ActivityCompleted(1)
	EndIf
EndEvent
