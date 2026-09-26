; Per-plant state for Powering Up (PowerPlantEvent 3E4E89). FO76 ran this on the server:
; it held the three plants' subsystem health, failed a plant when a subsystem dropped below
; CONST_SubsystemFailureThresholdPercent, and pushed PowerPlantEventQuestKeyword into the
; Story Manager (PowerPlantEventNode 3EB680) with the plant location as Location1 when a
; player approached. The event quest is event scoped (ENAM 'SCPT'), so the story event is the
; only way to start it.
;
; Single-player substitutions, all measured in game time so they survive save/load and sleep:
;   PowerPlantEventSuccessCooldown_Hours (8)  -> 8 in-game hours after a restart before the
;                                               plant breaks down again (FO76: 8 real hours).
;   PowerPlantEventFailureCooldown_Minutes(5) -> 5 in-game minutes before a failed event may
;                                               be offered again.
;   PowerPlantEventFailedStartupCooldown_Seconds (30) stays real time: it only throttles retries
;                                               when the story event did not start the quest.

Float Function GetGameTimeDays()
	Return Utility.GetCurrentGameTime()
EndFunction

Bool Function IsValidPowerPlantIndex(Int aiIndex)
	Return PowerPlantData != None && aiIndex >= 0 && aiIndex < PowerPlantData.Length
EndFunction

Int Function GetPowerPlantCount()
	If PowerPlantData == None
		Return 0
	EndIf
	Return PowerPlantData.Length
EndFunction

Int Function GetPowerPlantIndexForLocation(Location akLocation)
	If akLocation == None || PowerPlantData == None
		Return -1
	EndIf
	Int index = 0
	While index < PowerPlantData.Length
		If PowerPlantData[index].PowerPlantLocation == akLocation
			Return index
		EndIf
		index += 1
	EndWhile
	Return -1
EndFunction

Location Function GetPowerPlantShortNameLocation(Int aiIndex)
	If !IsValidPowerPlantIndex(aiIndex)
		Return None
	EndIf
	Return PowerPlantData[aiIndex].PowerPlantLocationShortName
EndFunction

Location Function GetPowerPlantTitleNameLocation(Int aiIndex)
	If !IsValidPowerPlantIndex(aiIndex)
		Return None
	EndIf
	Return PowerPlantData[aiIndex].PowerPlantLocationTitleName
EndFunction

Bool Function IsPowerPlantActive(Int aiIndex)
	If !IsValidPowerPlantIndex(aiIndex)
		Return False
	EndIf
	Return PowerPlantData[aiIndex].isActive
EndFunction

Int Function GetSubsystemIntactPercent(Int aiIndex, Int aiSubsystemID)
	If !IsValidPowerPlantIndex(aiIndex)
		Return 0
	EndIf
	If aiSubsystemID == 0
		Return PowerPlantData[aiIndex].PowerPlantData_SubsystemReactorIntactPercent
	ElseIf aiSubsystemID == 1
		Return PowerPlantData[aiIndex].PowerPlantData_SubsystemGeneratorIntactPercent
	ElseIf aiSubsystemID == 2
		Return PowerPlantData[aiIndex].PowerPlantData_SubsystemCoolingIntactPercent
	EndIf
	Return 0
EndFunction

Function SetSubsystemIntactPercent(Int aiIndex, Int aiSubsystemID, Int aiPercent, Bool abHasFailed)
	If !IsValidPowerPlantIndex(aiIndex)
		Return
	EndIf
	If aiSubsystemID == 0
		PowerPlantData[aiIndex].PowerPlantData_SubsystemReactorIntactPercent = aiPercent
		PowerPlantData[aiIndex].PowerPlantData_SubsystemReactorHasFailed = abHasFailed
	ElseIf aiSubsystemID == 1
		PowerPlantData[aiIndex].PowerPlantData_SubsystemGeneratorIntactPercent = aiPercent
		PowerPlantData[aiIndex].PowerPlantData_SubsystemGeneratorHasFailed = abHasFailed
	ElseIf aiSubsystemID == 2
		PowerPlantData[aiIndex].PowerPlantData_SubsystemCoolingIntactPercent = aiPercent
		PowerPlantData[aiIndex].PowerPlantData_SubsystemCoolingHasFailed = abHasFailed
	EndIf
	PowerPlantData[aiIndex].PowerPlantData_AnySubsystemHasFailed = PowerPlantData[aiIndex].PowerPlantData_SubsystemReactorHasFailed || PowerPlantData[aiIndex].PowerPlantData_SubsystemGeneratorHasFailed || PowerPlantData[aiIndex].PowerPlantData_SubsystemCoolingHasFailed
EndFunction

powerplanteventquestscript Function GetActivePowerPlantEvent(Int aiIndex)
	If !IsValidPowerPlantIndex(aiIndex)
		Return None
	EndIf
	powerplanteventquestscript activeEvent = PowerPlantData[aiIndex].ActivePowerPlantEvent
	If activeEvent == None
		Return None
	EndIf
	If !activeEvent.IsRunning()
		Return None
	EndIf
	Return activeEvent
EndFunction

Function RegisterPowerPlantEvent(Int aiIndex, powerplanteventquestscript akEvent)
	If !IsValidPowerPlantIndex(aiIndex)
		Return
	EndIf
	PowerPlantData[aiIndex].ActivePowerPlantEvent = akEvent
	PowerPlantData[aiIndex].PowerPlantEventStartupFailureTimestamp = -1.0
	PowerPlantData[aiIndex].lock = True
EndFunction

Function NotifyPowerPlantEventStopped(Int aiIndex)
	If !IsValidPowerPlantIndex(aiIndex)
		Return
	EndIf
	PowerPlantData[aiIndex].ActivePowerPlantEvent = None
	PowerPlantData[aiIndex].lock = False
EndFunction

Function NotifyPowerPlantRestarted(Int aiIndex)
	If !IsValidPowerPlantIndex(aiIndex)
		Return
	EndIf
	PowerPlantData[aiIndex].isActive = True
	PowerPlantData[aiIndex].isExitingFailureCooldown = False
	PowerPlantData[aiIndex].PowerPlantEventSuccessTimestamp = GetGameTimeDays()
	PowerPlantData[aiIndex].PowerPlantEventFailureTimestamp = -1.0
	PowerPlantData[aiIndex].PowerPlantEventStartupFailureTimestamp = -1.0
	SetSubsystemIntactPercent(aiIndex, 0, 100, False)
	SetSubsystemIntactPercent(aiIndex, 1, 100, False)
	SetSubsystemIntactPercent(aiIndex, 2, 100, False)
EndFunction

Function NotifyPowerPlantEventFailed(Int aiIndex)
	If !IsValidPowerPlantIndex(aiIndex)
		Return
	EndIf
	PowerPlantData[aiIndex].isActive = False
	PowerPlantData[aiIndex].isExitingFailureCooldown = True
	PowerPlantData[aiIndex].PowerPlantEventFailureTimestamp = GetGameTimeDays()
EndFunction

; The plant breaks down again once the success cooldown has run out. The stored percentages go
; to zero; the event clamps them up to its own CONST_SubsystemFailureAllowedPercent floor when it
; damages the world, so a plant that failed mid-repair keeps the progress it had.
Function MarkPowerPlantFailed(Int aiIndex)
	If !IsValidPowerPlantIndex(aiIndex) || !PowerPlantData[aiIndex].isActive
		Return
	EndIf
	PowerPlantData[aiIndex].isActive = False
	PowerPlantData[aiIndex].isExitingFailureCooldown = False
	PowerPlantData[aiIndex].PowerPlantEventSuccessTimestamp = -1.0
	PowerPlantData[aiIndex].PowerPlantEventFailureTimestamp = -1.0
	SetSubsystemIntactPercent(aiIndex, 0, 0, True)
	SetSubsystemIntactPercent(aiIndex, 1, 0, True)
	SetSubsystemIntactPercent(aiIndex, 2, 0, True)
EndFunction

Bool Function IsPowerPlantEventAvailable(Int aiIndex)
	If !IsValidPowerPlantIndex(aiIndex) || PowerPlantData[aiIndex].isActive
		Return False
	EndIf
	If GetActivePowerPlantEvent(aiIndex) != None
		Return False
	EndIf
	If PowerPlantEvent == None || PowerPlantEventQuestKeyword == None
		Return False
	EndIf
	Float now = GetGameTimeDays()
	Float failedAt = PowerPlantData[aiIndex].PowerPlantEventFailureTimestamp
	If failedAt >= 0.0 && now < failedAt + PowerPlantEventFailureCooldown_Minutes / 1440.0
		Return False
	EndIf
	Float startupFailedAt = PowerPlantData[aiIndex].PowerPlantEventStartupFailureTimestamp
	If startupFailedAt >= 0.0 && Utility.GetCurrentRealTime() < startupFailedAt + PowerPlantEventFailedStartupCooldown_Seconds
		Return False
	EndIf
	Return True
EndFunction

Bool Function IsPlayerNearPowerPlant(Int aiIndex)
	If !IsValidPowerPlantIndex(aiIndex)
		Return False
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return False
	EndIf
	Float enterRadius = CONST_PowerPlantEventActivityEnterRadius as Float
	ObjectReference exteriorRoot = PowerPlantData[aiIndex].PowerPlantEventRootExterior
	If exteriorRoot != None && playerRef.GetDistance(exteriorRoot) <= enterRadius
		Return True
	EndIf
	ObjectReference interiorRoot = PowerPlantData[aiIndex].PowerPlantEventRootInterior
	Return interiorRoot != None && playerRef.GetDistance(interiorRoot) <= enterRadius
EndFunction

; Story Manager start. Keyword.SendStoryEvent returns nothing in FO4, so the attempt is recorded
; as a failure up front and cleared by RegisterPowerPlantEvent when the quest actually starts.
Function TrySendPowerPlantEvent(Int aiIndex)
	If !IsValidPowerPlantIndex(aiIndex) || PowerPlantEventQuestKeyword == None
		Return
	EndIf
	PowerPlantData[aiIndex].PowerPlantEventStartupFailureTimestamp = Utility.GetCurrentRealTime()
	PowerPlantEventQuestKeyword.SendStoryEvent(PowerPlantData[aiIndex].PowerPlantLocation, None, None, aiIndex, 0)
EndFunction

Function UpdatePowerPlants()
	Int count = GetPowerPlantCount()
	Int index = 0
	While index < count
		If PowerPlantData[index].isActive && PowerPlantData[index].PowerPlantEventSuccessTimestamp >= 0.0
			If GetGameTimeDays() >= PowerPlantData[index].PowerPlantEventSuccessTimestamp + PowerPlantEventSuccessCooldown_Hours / 24.0
				MarkPowerPlantFailed(index)
			EndIf
		EndIf
		index += 1
	EndWhile

	; One plant at a time, exactly like FO76's "unless it is already active in another location".
	If PowerPlantEvent != None && PowerPlantEvent.IsRunning()
		Return
	EndIf
	index = 0
	While index < count
		If IsPowerPlantEventAvailable(index) && IsPlayerNearPowerPlant(index)
			TrySendPowerPlantEvent(index)
			Return
		EndIf
		index += 1
	EndWhile
EndFunction

Function ArmPowerPlantUpdateTimer()
	CancelTimer(CONST_PowerPlantUpdateTimerID)
	StartTimer(CONST_PowerPlantUpdateTimerDelay as Float, CONST_PowerPlantUpdateTimerID)
EndFunction

Event OnQuestInit()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	ArmPowerPlantUpdateTimer()
EndEvent

; This quest is StartGameEnabled, so OnQuestInit runs once ever: the proximity poll is re-armed on
; load as well rather than trusting a saved timer.
Event Actor.OnPlayerLoadGame(Actor akSender)
	If IsRunning()
		ArmPowerPlantUpdateTimer()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != CONST_PowerPlantUpdateTimerID
		Return
	EndIf
	If !IsRunning()
		Return
	EndIf
	UpdatePowerPlants()
	ArmPowerPlantUpdateTimer()
EndEvent

Event OnQuestShutdown()
	CancelTimer(CONST_PowerPlantUpdateTimerID)
EndEvent
