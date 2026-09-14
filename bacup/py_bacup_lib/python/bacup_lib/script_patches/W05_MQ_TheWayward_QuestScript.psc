; The Wayward's interior dialogue quest is StartGameEnabled, so QuestInitStage
; is only meaningful once the player is actually inside WaywardLocation. The
; player's location is tracked from OnQuestInit onward, and Duchess' "angry"
; alias plus the weekly bullion counter are refreshed from the bound reputation
; globals and timestamp actor values.
Function RefreshDuchessAngryState()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || DuchessAngryAlias == None || Duchess == None
		Return
	EndIf

	Bool bAngry = False
	If Reputation_AV_Foundation != None && Rep_Tier_Foundation_0_Hostile != None
		If playerRef.GetValue(Reputation_AV_Foundation) <= Rep_Tier_Foundation_0_Hostile.GetValue()
			bAngry = True
		EndIf
	EndIf
	If !bAngry && Reputation_AV_Crater != None && Rep_Tier_Crater_0_Hostile != None
		If playerRef.GetValue(Reputation_AV_Crater) <= Rep_Tier_Crater_0_Hostile.GetValue()
			bAngry = True
		EndIf
	EndIf
	If !bAngry && W05_MQ_004P_Crane_BadEnding != None && W05_MQ_004P_Crane_DuchessCooldown != None
		If playerRef.GetValue(W05_MQ_004P_Crane_BadEnding) > 0.0
			bAngry = W05_MQ_004P_Crane_DuchessCooldown.GetValue() > 0.0
		EndIf
	EndIf

	Actor duchessRef = Duchess.GetActorReference()
	ObjectReference angryRef = DuchessAngryAlias.GetReference()
	If bAngry
		If duchessRef != None && angryRef != (duchessRef as ObjectReference)
			DuchessAngryAlias.ForceRefTo(duchessRef)
		EndIf
	ElseIf angryRef != None
		DuchessAngryAlias.Clear()
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
	If akLoc != None && WaywardLocation != None && akLoc == WaywardLocation
		If !Self.GetStageDone(QuestInitStage)
			Self.SetStage(QuestInitStage)
		EndIf
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
	EvaluateWaywardPresence(playerRef.GetCurrentLocation())
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
	If akSender == Game.GetPlayer()
		EvaluateWaywardPresence(akNewLoc)
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	RefreshDuchessAngryState()
EndEvent
