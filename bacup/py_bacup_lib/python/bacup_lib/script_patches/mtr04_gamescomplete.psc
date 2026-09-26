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
	PublishCalibrationCount(0)
EndFunction

Function PublishCalibrationCount(Int aiCompletedActivities)
	Quest owner = Self as Quest
	B21:QuestVariables questVariables = owner as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable("GamesCalibrated", aiCompletedActivities as Float)
	EndIf
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
	PublishCalibrationCount(completedActivities)

	If completedActivities >= NumActivities && IsStageDone(PrerequisiteStage) && !IsStageDone(StageToSet)
		SetStage(StageToSet)
	EndIf
EndFunction

Function ScheduleSecurityWelcome()
	; Literal ids: the script may not declare new timer variables.
	StartTimer(10.0, 92)
EndFunction

Function ScheduleBossMeeting()
	StartTimer(10.0, 93)
EndFunction

Function ResolveBossMeeting()
	If !IsStageDone(1000) || IsStageDone(2000)
		Return
	EndIf
	If playerRef == None
		playerRef = OwningPlayer.GetActorReference()
	EndIf

	If playerRef != None && Uniform != None && playerRef.IsEquipped(Uniform)
		SetStage(2000)
	ElseIf !IsStageDone(1001)
		SetStage(1001)
	EndIf
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 92
		If IsStageDone(200) && !IsStageDone(300)
			SetStage(300)
		EndIf
	ElseIf aiTimerID == 93
		ResolveBossMeeting()
	EndIf
EndEvent

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
		If Boss != None
			Boss.Start()
		EndIf
		ScheduleBossMeeting()
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(92)
	CancelTimer(93)
	UnregisterForAllEvents()
	RemoveAllInventoryEventFilters()
EndEvent
