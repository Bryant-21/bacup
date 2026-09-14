Function StartDailyQuestPulse()
	Float pulseHours = 1.0
	If SQ_RegionManagerPulseTimeSeconds != None
		Float pulseSeconds = SQ_RegionManagerPulseTimeSeconds.GetValue()
		If pulseSeconds > 0.0
			pulseHours = pulseSeconds / 3600.0
		EndIf
	EndIf

	StartTimerGameTime(pulseHours, pulseDelayTimerID)
	Debug.Trace("[B21 Daily] RegionManager armed game-time pulse hours=" + pulseHours as String + " timerID=" + pulseDelayTimerID as String, 0)
EndFunction

Function RegisterDailyQuestEvents()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnLocationChange")
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
		Debug.Trace("[B21 Daily] RegionManager registered player events player=" + playerRef as String, 0)
	Else
		Debug.Trace("[B21 Daily] RegionManager could not register player events: Game.GetPlayer returned None", 0)
	EndIf

	StartDailyQuestPulse()
EndFunction

Function RunDailyQuestController(Location akPlayerLocation)
	Debug.Trace("[B21 Daily] RegionManager controller tick location=" + akPlayerLocation as String + " SQ_Master=" + SQ_Master as String + " keyword=" + SQ_RegionDailyQuestKeyword as String, 0)
	SQ_MasterScript masterController = SQ_Master as SQ_MasterScript
	If masterController != None
		masterController.UpdateDailyQuestSchedule()
		masterController.TryStartDailyQuest(akPlayerLocation, SQ_RegionDailyQuestKeyword)
		Debug.Trace("[B21 Daily] RegionManager controller tick completed", 0)
	Else
		Debug.Trace("[B21 Daily] RegionManager SQ_Master cast failed boundQuest=" + SQ_Master as String, 0)
	EndIf
EndFunction

Event OnQuestInit()
	Debug.Trace("[B21 Daily] RegionManager OnQuestInit", 0)
	RegisterDailyQuestEvents()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		RunDailyQuestController(playerRef.GetCurrentLocation())
	Else
		Debug.Trace("[B21 Daily] RegionManager OnQuestInit could not run controller: player is None", 0)
	EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	Debug.Trace("[B21 Daily] RegionManager OnPlayerLoadGame sender=" + akSender as String, 0)
	RegisterDailyQuestEvents()
	RunDailyQuestController(akSender.GetCurrentLocation())
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
	Debug.Trace("[B21 Daily] RegionManager OnLocationChange old=" + akOldLoc as String + " new=" + akNewLoc as String, 0)
	RunDailyQuestController(akNewLoc)
EndEvent

Event OnTimerGameTime(Int aiTimerID)
	If aiTimerID == pulseDelayTimerID
		Debug.Trace("[B21 Daily] RegionManager game-time pulse fired timerID=" + aiTimerID as String, 0)
		Actor playerRef = Game.GetPlayer()
		If playerRef != None
			RunDailyQuestController(playerRef.GetCurrentLocation())
		Else
			Debug.Trace("[B21 Daily] RegionManager pulse could not run controller: player is None", 0)
		EndIf
		StartDailyQuestPulse()
	EndIf
EndEvent
