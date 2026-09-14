Event OnQuestInit()
	EnsureLocalModuleLocation()
	InitializeLocalDefendStages()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If Alias_ModuleExpeditionTeam != None && Alias_ModuleExpeditionTeam.Find(playerRef) < 0
			Alias_ModuleExpeditionTeam.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	RegisterLocalDefendNPC()
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
	CancelTimer(PROGRESS_TIMER)
	UnregisterForAllEvents()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer() && !IsStageDone(CompleteStage)
		RegisterLocalDefendNPC()
		If IsStageDone(BeginActivitiesStage)
			ResumeLocalDefendActivity()
		EndIf
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	If akActivator != Game.GetPlayer() || akSender != NPC_Alias.GetReference()
		Return
	EndIf
	If !IsStageDone(BeginActivitiesStage)
		BeginLocalDefendActivities()
	ElseIf NPC_Actor != None && (NPC_Actor.IsDead() || NPC_Actor.GetValue(Health) <= 0.0)
		ReviveLocalDefendNPC()
	EndIf
EndEvent

Event Actor.OnEnterBleedout(Actor akSender)
	If akSender == NPC_Actor
		SetObjectiveDisplayed(Revive_NPC_Obj, True)
		If ReviveNPC_AnnounceMessage != None
			ReviveNPC_AnnounceMessage.Show()
		EndIf
	EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
	If akSender == NPC_Actor
		SetObjectiveDisplayed(Revive_NPC_Obj, True)
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == BeginActivitiesStage && !DefendComplete
		BeginLocalDefendActivities()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != PROGRESS_TIMER || DefendComplete || IsStageDone(CompleteStage) || !IsStageDone(BeginActivitiesStage)
		Return
	EndIf
	If NPC_Actor == None
		RegisterLocalDefendNPC()
	EndIf
	If NPC_Actor == None || NPC_Actor.IsDead() || NPC_Actor.GetValue(Health) <= 0.0
		SetObjectiveDisplayed(Revive_NPC_Obj, True)
		StartTimer(PROGRESS_INTERVAL, PROGRESS_TIMER)
		Return
	EndIf
	If HaltProgressWhenPlayersTooFarFromNPC && IsLocalPlayerTooFarFromNPC()
		SetObjectiveDisplayed(ReturnToNPC_Obj, True)
		StartTimer(PROGRESS_INTERVAL, PROGRESS_TIMER)
		Return
	EndIf
	SetObjectiveDisplayed(ReturnToNPC_Obj, False)
	SetObjectiveDisplayed(Revive_NPC_Obj, False)

	Float oldProgress = OverallProgress
	CurrentActivity.SecsProgress += PROGRESS_INTERVAL
	Float activityGoal = CurrentActivity.SecsToComplete
	If activityGoal <= 0.0
		activityGoal = SecsToCompleteEachActivity
	EndIf
	If activityGoal <= 0.0
		activityGoal = 1.0
	EndIf
	OverallProgress = CountCompletedLocalActivities() * activityGoal + CurrentActivity.SecsProgress
	ApplyLocalDefendThresholds(oldProgress, OverallProgress)
	If CurrentActivity.SecsProgress >= activityGoal
		CompleteCurrentLocalDefendActivity()
	Else
		StartTimer(PROGRESS_INTERVAL, PROGRESS_TIMER)
	EndIf
EndEvent

Function InitializeLocalDefendStages()
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

Function RegisterLocalDefendNPC()
	NPC_Actor = NPC_Alias.GetReference() as Actor
	If NPC_Actor == None
		Return
	EndIf
	If EnableNPCOnMissionStart || EnableNPCOnModuleActive
		NPC_Actor.EnableNoWait()
	EndIf
	RegisterForRemoteEvent(NPC_Actor, "OnActivate")
	RegisterForRemoteEvent(NPC_Actor, "OnEnterBleedout")
	RegisterForRemoteEvent(NPC_Actor, "OnDeath")
EndFunction

Function BeginLocalDefendActivities()
	If DefendComplete || IsStageDone(CompleteStage)
		Return
	EndIf
	If !IsStageDone(BeginActivitiesStage)
		SetStage(BeginActivitiesStage)
	EndIf
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(400, True)
	If NPCHealthBarObjID >= 0
		SetObjectiveDisplayed(NPCHealthBarObjID, True)
	EndIf
	EWS_Script = (Self as Quest) as defaultquestencounterwavescript
	OverallProgressGoal = GetLocalDefendGoal() * SecsToCompleteEachActivity
	ResumeLocalDefendActivity()
EndFunction

Function ResumeLocalDefendActivity()
	If DefendComplete || IsStageDone(CompleteStage)
		Return
	EndIf
	Int completedCount = CountCompletedLocalActivities()
	If completedCount >= GetLocalDefendGoal() && GetLocalDefendGoal() > 0
		CompleteLocalDefendModule()
		Return
	EndIf
	SetsCompletedStagesIndex = GetNextLocalDefendActivityIndex(0)
	If SetsCompletedStagesIndex < 0
		Return
	EndIf
	CurrentActivity = NPC_ActivitySets[SetsCompletedStagesIndex]
	PrepareCurrentLocalDefendActivity()
EndFunction

Function PrepareCurrentLocalDefendActivity()
	If NPC_Actor == None
		RegisterLocalDefendNPC()
	EndIf
	If NPC_Actor == None
		Return
	EndIf
	CurrentActivity.ActivityIdleMarker_Refr = CurrentActivity.ActivityIdleMarker_Alias.GetReference()
	CurrentActivity.ActivityFurniture_Refr = CurrentActivity.ActivityFurniture_Alias.GetReference()
	If CurrentActivity.ActivityIdleMarker_Refr != None
		NPC_Actor.MoveTo(CurrentActivity.ActivityIdleMarker_Refr)
	EndIf
	If !IsStageDone(FirstActivityStartedStage)
		SetStage(FirstActivityStartedStage)
	EndIf
	If CurrentActivity.StageToSetOnThisActivityStarted >= 0 && !IsStageDone(CurrentActivity.StageToSetOnThisActivityStarted)
		SetStage(CurrentActivity.StageToSetOnThisActivityStarted)
	EndIf
	If !CurrentActivity.WaveStarted && EWS_Script != None && CurrentActivity.WaveToStart >= 0
		EWS_Script.StartLocalEncounterWave(CurrentActivity.WaveToStart)
		CurrentActivity.WaveStarted = True
	EndIf
	EnableLocalDefendLinkedRef(CurrentActivity.ActivityIdleMarker_Refr, XPD_ObjMod_DefendNPC_LinkEnableOnStart_Keyword)
	EnableLocalDefendLinkedRef(CurrentActivity.ActivityFurniture_Refr, XPD_ObjMod_DefendNPC_LinkEnableOnStart_Keyword)
	NPC_Actor.EvaluatePackage()
	CancelTimer(PROGRESS_TIMER)
	StartTimer(PROGRESS_INTERVAL, PROGRESS_TIMER)
EndFunction

Function CompleteCurrentLocalDefendActivity()
	If CurrentActivity.StageToSetOnActivityComplete >= 0 && !IsStageDone(CurrentActivity.StageToSetOnActivityComplete)
		SetStage(CurrentActivity.StageToSetOnActivityComplete)
	EndIf
	EnableLocalDefendLinkedRef(CurrentActivity.ActivityIdleMarker_Refr, XPD_ObjMod_DefendNPC_LinkEnableOnCompletion_Keyword)
	EnableLocalDefendLinkedRef(CurrentActivity.ActivityFurniture_Refr, XPD_ObjMod_DefendNPC_LinkEnableOnCompletion_Keyword)
	Int completedCount = CountCompletedLocalActivities()
	If SetsCompletedStages != None && completedCount > 0 && completedCount <= SetsCompletedStages.Length
		Int completedStage = SetsCompletedStages[completedCount - 1]
		If completedStage >= 0 && !IsStageDone(completedStage)
			SetStage(completedStage)
		EndIf
	EndIf
	If completedCount >= GetLocalDefendGoal()
		CompleteLocalDefendModule()
	Else
		ResumeLocalDefendActivity()
	EndIf
EndFunction

Function ReviveLocalDefendNPC()
	If NPC_Actor == None
		Return
	EndIf
	If NPC_Actor.IsDead()
		NPC_Actor.Resurrect()
	EndIf
	Float healthToRestore = NPC_Actor.GetBaseValue(Health)
	If healthToRestore <= 0.0
		healthToRestore = 100.0
	EndIf
	NPC_Actor.RestoreValue(Health, healthToRestore)
	SetObjectiveDisplayed(Revive_NPC_Obj, False)
	StartTimer(PROGRESS_INTERVAL, PROGRESS_TIMER)
EndFunction

Function ApplyLocalDefendThresholds(Float oldProgress, Float newProgress)
	Float oldPercent = 0.0
	Float newPercent = 100.0
	If OverallProgressGoal > 0.0
		oldPercent = oldProgress / OverallProgressGoal * 100.0
		newPercent = newProgress / OverallProgressGoal * 100.0
	EndIf
	Int thresholdIndex = 0
	While ProgressTresholds != None && thresholdIndex < ProgressTresholds.Length
		ProgressTresholdData thresholdData = ProgressTresholds[thresholdIndex]
		If oldPercent < thresholdData.ProgressThreshold_Percent && newPercent >= thresholdData.ProgressThreshold_Percent
			If thresholdData.MissionQuestStageToSet >= 0 && !IsStageDone(thresholdData.MissionQuestStageToSet)
				SetStage(thresholdData.MissionQuestStageToSet)
			EndIf
			If EWS_Script != None && thresholdData.WaveToStart >= 0
				EWS_Script.StartLocalEncounterWave(thresholdData.WaveToStart)
			EndIf
		EndIf
		thresholdIndex += 1
	EndWhile
EndFunction

Function EnableLocalDefendLinkedRef(ObjectReference sourceRef, Keyword linkKeyword)
	If sourceRef == None || linkKeyword == None
		Return
	EndIf
	ObjectReference linkedRef = sourceRef.GetLinkedRef(linkKeyword)
	If linkedRef != None
		linkedRef.EnableNoWait()
	EndIf
EndFunction

Bool Function IsLocalPlayerTooFarFromNPC()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || NPC_Actor == None || XPD_ObjMod_DefendNPC_PlayerDistanceToNPCRequired == None
		Return False
	EndIf
	Return playerRef.GetDistance(NPC_Actor) > XPD_ObjMod_DefendNPC_PlayerDistanceToNPCRequired.GetValue()
EndFunction

Int Function GetNextLocalDefendActivityIndex(Int startIndex)
	Int activityIndex = startIndex
	While NPC_ActivitySets != None && activityIndex < NPC_ActivitySets.Length
		NPC_ActivitySetData activityData = NPC_ActivitySets[activityIndex]
		Bool hasTarget = activityData.ActivityIdleMarker_Alias.GetReference() != None || activityData.ActivityFurniture_Alias.GetReference() != None
		Bool isComplete = activityData.StageToSetOnActivityComplete >= 0 && IsStageDone(activityData.StageToSetOnActivityComplete)
		If hasTarget && !isComplete
			Return activityIndex
		EndIf
		activityIndex += 1
	EndWhile
	Return -1
EndFunction

Int Function CountCompletedLocalActivities()
	Int completedCount = 0
	Int activityIndex = 0
	While NPC_ActivitySets != None && activityIndex < NPC_ActivitySets.Length
		NPC_ActivitySetData activityData = NPC_ActivitySets[activityIndex]
		If activityData.StageToSetOnActivityComplete >= 0 && IsStageDone(activityData.StageToSetOnActivityComplete)
			completedCount += 1
		EndIf
		activityIndex += 1
	EndWhile
	Return completedCount
EndFunction

Int Function GetLocalDefendGoal()
	Int validCount = 0
	Int activityIndex = 0
	While NPC_ActivitySets != None && activityIndex < NPC_ActivitySets.Length
		NPC_ActivitySetData activityData = NPC_ActivitySets[activityIndex]
		If activityData.ActivityIdleMarker_Alias.GetReference() != None || activityData.ActivityFurniture_Alias.GetReference() != None
			validCount += 1
		EndIf
		activityIndex += 1
	EndWhile
	If NumSetsToComplete > 0 && NumSetsToComplete < validCount
		validCount = NumSetsToComplete
	EndIf
	Return validCount
EndFunction

Function CompleteLocalDefendModule()
	If DefendComplete || IsStageDone(CompleteStage)
		Return
	EndIf
	DefendComplete = True
	CancelTimer(PROGRESS_TIMER)
	SetObjectiveCompleted(400, True)
	If NPCHealthBarObjID >= 0
		SetObjectiveCompleted(NPCHealthBarObjID, True)
	EndIf
	SetStage(CompleteStage)
EndFunction
