Event OnQuestInit()
	playerRef = OwningPlayer.GetActorReference()
	ResetActivityCompletion()
	RemoveAllInventoryEventFilters()
	If playerRef != None
		If Uniform != None
			AddInventoryEventFilter(Uniform)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnItemAdded")
		RegisterForRemoteEvent(playerRef, "OnItemEquipped")
	EndIf
EndEvent

Function ResetActivityCompletion()
	ActivityCompletionStatus = new Bool[NumActivities]
EndFunction

Function ActivityCompleted(Int aiActivityIndex)
	If ActivityCompletionStatus == None || ActivityCompletionStatus.Length != NumActivities
		ResetActivityCompletion()
	EndIf
	If aiActivityIndex < 0 || aiActivityIndex >= ActivityCompletionStatus.Length
		Return
	EndIf

	ActivityCompletionStatus[aiActivityIndex] = True
	Int completedActivities = 0
	Int activityIndex = 0
	While activityIndex < ActivityCompletionStatus.Length
		If ActivityCompletionStatus[activityIndex]
			completedActivities += 1
		EndIf
		activityIndex += 1
	EndWhile

	If completedActivities >= NumActivities && IsStageDone(PrerequisiteStage) && !IsStageDone(StageToSet)
		SetStage(StageToSet)
	EndIf
EndFunction

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If akSender == playerRef && akBaseItem == Uniform && IsStageDone(300) && !IsStageDone(400)
		SetStage(400)
	EndIf
EndEvent

Event Actor.OnItemEquipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
	If akSender != playerRef || akBaseObject != Uniform
		Return
	EndIf
	If IsStageDone(300) && !IsStageDone(700)
		SetStage(700)
	ElseIf IsStageDone(1001) && !IsStageDone(2000)
		Boss.Start()
	EndIf
EndEvent

Event OnQuestShutdown()
	UnregisterForAllEvents()
	RemoveAllInventoryEventFilters()
EndEvent
