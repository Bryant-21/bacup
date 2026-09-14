Event OnQuestInit()
	EnsureLocalModuleLocation()
	InitializeLocalAssassinationStages()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If ModuleExpeditionTeam_RefCollAlias != None && ModuleExpeditionTeam_RefCollAlias.Find(playerRef) < 0
			ModuleExpeditionTeam_RefCollAlias.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	PrepareLocalAssassination()
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
	UnregisterForAllEvents()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer() && !IsStageDone(CompleteStage)
		PrepareLocalAssassination()
	EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
	ObjectReference leaderRef = AssassinationSetLeader_Alias.GetReference()
	If akSender == leaderRef
		CompleteLocalAssassination()
	EndIf
EndEvent

Function InitializeLocalAssassinationStages()
	WaitingStage = XPD_ObjectiveModule_WaitingStage_Global.GetValue() as Int
	ActiveStage = XPD_ObjectiveModule_ActiveStage_Global.GetValue() as Int
	CompleteStage = XPD_ObjectiveModule_CompleteStage_Global.GetValue() as Int
	If !IsStageDone(WaitingStage)
		SetStage(WaitingStage)
	EndIf
	If !IsStageDone(ActiveStage)
		SetStage(ActiveStage)
	EndIf
EndFunction

Function PrepareLocalAssassination()
	If IsStageDone(CompleteStage) || AssassinationSets == None || AssassinationSets.Length == 0
		Return
	EndIf

	CurrentAssassinationSet = 0
	AssassinationSetData setData = AssassinationSets[CurrentAssassinationSet]
	If Territories_RefCollAlias != None && Territories_RefCollAlias.GetCount() > 0
		setData.RandomlyChosenTerritory = Territories_RefCollAlias.GetAt(0)
		If CurrentTerritory_Alias.GetReference() != setData.RandomlyChosenTerritory
			CurrentTerritory_Alias.ForceRefTo(setData.RandomlyChosenTerritory)
		EndIf
	EndIf

	SetObjectiveCompleted(200, True)
	If setData.AssassinationTargetSpawnedStage >= 0
		If !IsStageDone(setData.AssassinationTargetSpawnedStage)
			SetStage(setData.AssassinationTargetSpawnedStage)
		EndIf
		SetObjectiveDisplayed(setData.AssassinationTargetSpawnedStage, True)
	EndIf

	EWS_Script = (Self as Quest) as defaultquestencounterwavescript
	If EWS_Script != None && setData.WaveIndex >= 0
		EWS_Script.StartLocalEncounterWave(setData.WaveIndex)
	EndIf
	RegisterLocalAssassinationTargets()
EndFunction

Function RegisterLocalAssassinationTargets()
	Actor leaderRef = AssassinationSetLeader_Alias.GetReference() as Actor
	If leaderRef != None
		If leaderRef.IsDead()
			CompleteLocalAssassination()
		Else
			leaderRef.EnableNoWait()
			RegisterForRemoteEvent(leaderRef, "OnDeath")
			Actor playerRef = Game.GetPlayer()
			If playerRef != None
				leaderRef.StartCombat(playerRef)
			EndIf
		EndIf
	EndIf

	Int followerIndex = 0
	While AssassinationSetFollowers_RefCollAlias != None && followerIndex < AssassinationSetFollowers_RefCollAlias.GetCount()
		Actor followerRef = AssassinationSetFollowers_RefCollAlias.GetActorAt(followerIndex)
		If followerRef != None && !followerRef.IsDead()
			RegisterForRemoteEvent(followerRef, "OnDeath")
		EndIf
		followerIndex += 1
	EndWhile
EndFunction

Function CompleteLocalAssassination()
	If IsStageDone(CompleteStage) || AssassinationSets == None || CurrentAssassinationSet < 0 || CurrentAssassinationSet >= AssassinationSets.Length
		Return
	EndIf

	AssassinationSetData setData = AssassinationSets[CurrentAssassinationSet]
	If setData.AssassinationTargetKilledStage >= 0 && !IsStageDone(setData.AssassinationTargetKilledStage)
		SetStage(setData.AssassinationTargetKilledStage)
	EndIf
	If setData.AssassinationTargetSpawnedStage >= 0
		SetObjectiveCompleted(setData.AssassinationTargetSpawnedStage, True)
	EndIf
	If XPD_ObjMod_Assassination_TargetAssassinated_Message != None
		XPD_ObjMod_Assassination_TargetAssassinated_Message.Show()
	EndIf
	SetStage(CompleteStage)
EndFunction
