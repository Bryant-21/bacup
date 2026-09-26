Function RefreshDuchessAngryState()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || DuchessAngryAlias == None || Duchess == None
		Return
	EndIf

	Bool bAngry = B21_WaywardState.BadEndingActive(playerRef, W05_MQ_004P_Crane_BadEnding, W05_MQ_004P_Crane_DuchessCooldown)

	Actor duchessRef = Duchess.GetActorReference()
	ObjectReference angryRef = DuchessAngryAlias.GetReference()
	If bAngry
		If duchessRef != None && angryRef != (duchessRef as ObjectReference)
			DuchessAngryAlias.ForceRefTo(duchessRef)
			duchessRef.EvaluatePackage()
		EndIf
	ElseIf angryRef != None
		DuchessAngryAlias.Clear()
		If duchessRef != None
			duchessRef.EvaluatePackage()
		EndIf
	EndIf
EndFunction

Function RefreshBullionWeek()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || GoldBullion_LastPurchasedTimestamp == None || W05_BullionPurchased == None
		Return
	EndIf

	Float nowDays = Utility.GetCurrentGameTime()
	Float stampDays = playerRef.GetValue(GoldBullion_LastPurchasedTimestamp)
	If stampDays <= 0.0
		playerRef.SetValue(GoldBullion_LastPurchasedTimestamp, nowDays)
		Return
	EndIf

	Bool bReset = (nowDays - stampDays) >= (DaysInAWeek as Float)
	If !bReset && (nowDays as Int) > (stampDays as Int)
		bReset = ((nowDays as Int) % DaysInAWeek) == DayOfWeekToResetBullion
	EndIf
	If bReset
		playerRef.SetValue(W05_BullionPurchased, 0.0)
		playerRef.SetValue(GoldBullion_LastPurchasedTimestamp, nowDays)
	EndIf
EndFunction

Function EvaluateWaywardPresence(Location akLoc)
	If !IsRunning()
		Return
	EndIf
	If akLoc != None && WaywardLocation != None && akLoc == WaywardLocation
		If !Self.GetStageDone(QuestInitStage)
			Self.SetStage(QuestInitStage)
		EndIf
		RefreshPollyAlias()
		RegisterWaywardTrigger()
		StartTimer(StareTimerLength, StareID)
	Else
		CancelTimer(StareID)
	EndIf

	RefreshDuchessAngryState()
	RefreshBullionWeek()
EndFunction

Event OnQuestInit()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf

	Self.RegisterForRemoteEvent(playerRef, "OnLocationChange")
	Self.RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EvaluateWaywardPresence(playerRef.GetCurrentLocation())
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
	If akSender == Game.GetPlayer()
		EvaluateWaywardPresence(akNewLoc)
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	RefreshDuchessAngryState()
	If auiStageID == 110
		StartTimer(StareTimerLength, StareID)
	EndIf
EndEvent

Actor Function GetEnabledPollyBody(Int aiFormID)
	Actor bodyRef = Game.GetFormFromFile(aiFormID, "SeventySix.esm") as Actor
	If bodyRef != None && !bodyRef.IsDisabled() && bodyRef.GetCurrentLocation() == WaywardLocation
		Return bodyRef
	EndIf
	Return None
EndFunction

Actor Function RefreshPollyAlias()
	Actor playerRef = Game.GetPlayer()
	If !IsRunning() || Polly == None || playerRef == None || WaywardLocation == None || playerRef.GetCurrentLocation() != WaywardLocation
		Return None
	EndIf
	Actor bodyRef = GetEnabledPollyBody(0x0042A255)
	If bodyRef == None
		bodyRef = GetEnabledPollyBody(0x0042A256)
	EndIf
	If bodyRef == None
		bodyRef = GetEnabledPollyBody(0x0042D278)
	EndIf
	If Polly.GetActorReference() != bodyRef
		If bodyRef != None
			Polly.ForceRefTo(bodyRef)
		Else
			Polly.Clear()
		EndIf
	EndIf
	Return bodyRef
EndFunction

Function RegisterWaywardTrigger()
	If WaywardInteriorTrigger != None && WaywardInteriorTrigger.GetReference() != None
		RegisterForRemoteEvent(WaywardInteriorTrigger.GetReference(), "OnTriggerEnter")
	EndIf
EndFunction

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	If !IsRunning() || akActionRef != Game.GetPlayer() || WaywardInteriorTrigger == None || akSender != WaywardInteriorTrigger.GetReference()
		Return
	EndIf
	If !IsStageDone(110)
		SetStage(110)
	EndIf
	StartTimer(StareTimerLength, StareID)
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer()
		EvaluateWaywardPresence(akSender.GetCurrentLocation())
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != StareID || !IsRunning()
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || WaywardLocation == None || playerRef.GetCurrentLocation() != WaywardLocation
		Return
	EndIf
	RefreshPollyAlias()
	RegisterWaywardTrigger()
	If W05_Wayward_PollyStartedIntro == None || playerRef.GetValue(W05_Wayward_PollyStartedIntro) != 0.0
		Return
	EndIf
	If IsStageDone(110)
		Fragments:Quests:QF_W05_DialogueTheWayward_0040F5BF fragments = (Self as Quest) as Fragments:Quests:QF_W05_DialogueTheWayward_0040F5BF
		If fragments != None
			fragments.TryStartPollyIntro()
		EndIf
	EndIf
	StartTimer(StareTimerLength, StareID)
EndEvent

Event OnQuestShutdown()
	CancelTimer(StareID)
	UnregisterForAllRemoteEvents()
EndEvent
