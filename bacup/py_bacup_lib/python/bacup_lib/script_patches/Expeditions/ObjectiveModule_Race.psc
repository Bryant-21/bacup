Event OnQuestInit()
	EnsureLocalModuleLocation()
	InitializeLocalRaceStages()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If Alias_ModuleExpeditionTeam != None && Alias_ModuleExpeditionTeam.Find(playerRef) < 0
			Alias_ModuleExpeditionTeam.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	PrepareLocalRace()
EndEvent

Function EnsureLocalModuleLocation()
	LocationAlias moduleLocation = GetAlias(3) as LocationAlias
	Actor playerRef = Game.GetPlayer()
	If moduleLocation != None && playerRef != None
		Location currentLocation = playerRef.GetCurrentLocation()
		If currentLocation != None && moduleLocation.GetLocation() != currentLocation
			moduleLocation.ForceLocationTo(currentLocation)
		EndIf
	EndIf
EndFunction

Event OnQuestShutdown()
	CancelTimer(1)
	UnregisterForAllEvents()
	CPArray.Clear()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer() && !IsStageDone(CompleteStage)
		If CPArray == None || CPArray.Length == 0
			PrepareLocalRace()
		Else
			RegisterLocalRaceCheckpoints()
		EndIf
		If RaceActive
			StartTimer(TimeLimit, 1)
		EndIf
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	If akActivator == Game.GetPlayer()
		HandleLocalRaceCheckpoint(akSender)
	EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		HandleLocalRaceCheckpoint(akSender)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 1 && RaceActive && !RaceCompleted
		RaceActive = False
		NextCP = 0
		SetObjectiveDisplayed(400, False)
		SetObjectiveDisplayed(300, True)
		If Message_TimesUp != None
			Message_TimesUp.Show()
		EndIf
		If ResetSound != None
			ResetSound.Play(Game.GetPlayer())
		EndIf
	EndIf
EndEvent

Function InitializeLocalRaceStages()
	WaitingStage = XPD_ObjectiveModule_WaitingStage_Global.GetValue() as Int
	ActiveStage = XPD_ObjectiveModule_ActiveStage_Global.GetValue() as Int
	CompleteStage = XPD_ObjectiveModule_CompleteStage_Global.GetValue() as Int
	If !IsStageDone(WaitingStage)
		SetStage(WaitingStage)
	EndIf
	If !IsStageDone(ActiveStage)
		SetStage(ActiveStage)
	EndIf
	SetObjectiveCompleted(200, True)
	SetObjectiveDisplayed(300, True)
EndFunction

Function PrepareLocalRace()
	If IsStageDone(CompleteStage) || RacePaths == None || RacePaths.Length == 0
		Return
	EndIf

	ChosenPath = RacePaths[0]
	FirstCPRef = ChosenPath.FirstCheckpoint.GetReference()
	If FirstCPRef == None
		Return
	EndIf
	If Alias_FirstCheckpoint_Chosen.GetReference() != FirstCPRef
		Alias_FirstCheckpoint_Chosen.ForceRefTo(FirstCPRef)
	EndIf
	If ChosenPath.EnableRefrs != None
		ChosenPath.EnableRefrs.EnableAll()
	EndIf
	If ChosenPath.ResetRefrs != None
		ChosenPath.ResetRefrs.ResetAll()
	EndIf

	CPArray = new ObjectReference[0]
	ObjectReference checkpointRef = FirstCPRef
	Int checkpointLimit = 128
	If Alias_AllCheckpoints != None && Alias_AllCheckpoints.GetCount() > 0
		checkpointLimit = Alias_AllCheckpoints.GetCount()
	EndIf
	While checkpointRef != None && CPArray.Length < checkpointLimit && CPArray.Find(checkpointRef) < 0
		CPArray.Add(checkpointRef)
		checkpointRef = checkpointRef.GetLinkedRef(XPD_ObjMod_Race_NextCheckpointKeyword)
	EndWhile
	If CPArray.Length == 1 && Alias_AllCheckpoints != None
		Int checkpointIndex = 0
		While checkpointIndex < Alias_AllCheckpoints.GetCount()
			checkpointRef = Alias_AllCheckpoints.GetAt(checkpointIndex)
			If checkpointRef != None && CPArray.Find(checkpointRef) < 0
				CPArray.Add(checkpointRef)
			EndIf
			checkpointIndex += 1
		EndWhile
	EndIf

	NextCP = 0
	RaceActive = False
	RaceCompleted = False
	RegisterLocalRaceCheckpoints()
EndFunction

Function RegisterLocalRaceCheckpoints()
	Int checkpointIndex = 0
	While CPArray != None && checkpointIndex < CPArray.Length
		ObjectReference checkpointRef = CPArray[checkpointIndex]
		If checkpointRef != None
			RegisterForRemoteEvent(checkpointRef, "OnActivate")
			RegisterForRemoteEvent(checkpointRef, "OnTriggerEnter")
		EndIf
		checkpointIndex += 1
	EndWhile
EndFunction

Function HandleLocalRaceCheckpoint(ObjectReference checkpointRef)
	If RaceCompleted || CPArray == None || NextCP < 0 || NextCP >= CPArray.Length || CPArray[NextCP] != checkpointRef
		Return
	EndIf

	If !RaceActive
		RaceActive = True
		SetObjectiveCompleted(Obj_StartRace, True)
		SetObjectiveDisplayed(Obj_CompleteRace, True)
		StartTimer(TimeLimit, 1)
	EndIf
	NextCP += 1
	If NextCP >= CPArray.Length
		RaceCompleted = True
		RaceActive = False
		CancelTimer(1)
		SetObjectiveCompleted(Obj_CompleteRace, True)
		SetStage(CompleteStage)
	EndIf
EndFunction
