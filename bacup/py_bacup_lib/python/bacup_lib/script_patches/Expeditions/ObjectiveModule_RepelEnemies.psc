Event OnQuestInit()
	EnsureLocalModuleLocation()
	InitializeLocalRepelStages()
	Actor playerRef = Game.GetPlayer()
	TeamLeader = playerRef
	If playerRef != None
		AddLocalRepelTeamMember(playerRef)
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	RegisterLocalRepelReferences()
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
	SecureAreaTriggers_Refr.Clear()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer() && !IsStageDone(CompleteStage)
		RegisterLocalRepelReferences()
		If IsStageDone(StageToSetOnStart) && !SecureComplete
			StartTimer(PROGRESS_INTERVAL, PROGRESS_TIMER)
		EndIf
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	If akActivator == Game.GetPlayer() && akSender == SecureAreaActivator_Alias.GetReference()
		StartLocalSecureArea()
	EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	Actor enemyRef = akActionRef as Actor
	If SecureAreaTriggers_Alias != None && SecureAreaTriggers_Alias.Find(akSender) >= 0 && IsLocalRepelEnemy(enemyRef)
		If EnemiesInSecureArea_RefCollAlias.Find(enemyRef) < 0
			EnemiesInSecureArea_RefCollAlias.AddRef(enemyRef)
		EndIf
		RegisterForRemoteEvent(enemyRef, "OnDeath")
	EndIf
EndEvent

Event ObjectReference.OnTriggerLeave(ObjectReference akSender, ObjectReference akActionRef)
	If SecureAreaTriggers_Alias != None && SecureAreaTriggers_Alias.Find(akSender) >= 0 && EnemiesInSecureArea_RefCollAlias != None
		EnemiesInSecureArea_RefCollAlias.RemoveRef(akActionRef)
	EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
	If EnemiesInSecureArea_RefCollAlias != None
		EnemiesInSecureArea_RefCollAlias.RemoveRef(akSender)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != PROGRESS_TIMER || SecureComplete || IsStageDone(CompleteStage) || !IsStageDone(StageToSetOnStart)
		Return
	EndIf
	ReconcileLocalSecureAreaEnemies()
	Float oldProgressPercent = 0.0
	If SecureGoalTime > 0.0
		oldProgressPercent = ProgressTime / SecureGoalTime * 100.0
	EndIf
	If EnemiesInSecureArea_RefCollAlias == None || EnemiesInSecureArea_RefCollAlias.GetCount() == 0
		ProgressTime += PROGRESS_INTERVAL
	EndIf
	Float progressPercent = 100.0
	If SecureGoalTime > 0.0
		progressPercent = ProgressTime / SecureGoalTime * 100.0
	EndIf
	ApplyLocalRepelThresholds(oldProgressPercent, progressPercent)
	If SecureAreaAct_Script != None
		SecureAreaAct_Script.UpdateProgress(progressPercent)
	EndIf
	If progressPercent >= 100.0
		CompleteLocalSecureArea()
	Else
		StartTimer(PROGRESS_INTERVAL, PROGRESS_TIMER)
	EndIf
EndEvent

Function InitializeLocalRepelStages()
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

Function AddLocalRepelTeamMember(Actor playerRef)
	If Alias_ModuleExpeditionTeam != None && Alias_ModuleExpeditionTeam.Find(playerRef) < 0
		Alias_ModuleExpeditionTeam.AddRef(playerRef)
	EndIf
	If ModuleExpeditionTeam_RefCollAlias != None && ModuleExpeditionTeam_RefCollAlias.Find(playerRef) < 0
		ModuleExpeditionTeam_RefCollAlias.AddRef(playerRef)
	EndIf
EndFunction

Function RegisterLocalRepelReferences()
	ObjectReference secureActivator = SecureAreaActivator_Alias.GetReference()
	If secureActivator != None
		secureActivator.EnableNoWait()
		RegisterForRemoteEvent(secureActivator, "OnActivate")
	EndIf
	ObjectReference activatorModel = SecureAreaActivatorModel_Alias.GetReference()
	If activatorModel == None
		activatorModel = secureActivator
	EndIf
	If activatorModel != None
		SecureAreaAct_Script = (activatorModel as ObjectReference) as Expeditions:ObjectiveModule_RepelEnemies_refr
	EndIf

	SecureAreaTriggers_Refr = new ObjectReference[0]
	Int triggerIndex = 0
	While SecureAreaTriggers_Alias != None && triggerIndex < SecureAreaTriggers_Alias.GetCount()
		ObjectReference triggerRef = SecureAreaTriggers_Alias.GetAt(triggerIndex)
		If triggerRef != None
			triggerRef.EnableNoWait()
			SecureAreaTriggers_Refr.Add(triggerRef)
			RegisterForRemoteEvent(triggerRef, "OnTriggerEnter")
			RegisterForRemoteEvent(triggerRef, "OnTriggerLeave")
		EndIf
		triggerIndex += 1
	EndWhile
	ReconcileLocalSecureAreaEnemies()
EndFunction

Function StartLocalSecureArea()
	If SecureComplete || IsStageDone(CompleteStage) || IsStageDone(StageToSetOnStart)
		Return
	EndIf
	SetStage(StageToSetOnStart)
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(SecureArea_Obj, True)
	ProgressTime = 0.0
	OldTimeStamp = Utility.GetCurrentRealTime()
	EWS_Script = (Self as Quest) as defaultquestencounterwavescript
	ApplyLocalRepelThresholds(-1.0, 0.0)
	StartTimer(PROGRESS_INTERVAL, PROGRESS_TIMER)
EndFunction

Bool Function IsLocalRepelEnemy(Actor enemyRef)
	If enemyRef == None || enemyRef.IsDead() || enemyRef == Game.GetPlayer()
		Return False
	EndIf
	If Enemies_RefCollAlias != None && Enemies_RefCollAlias.Find(enemyRef) >= 0
		Return True
	EndIf
	Return enemyRef.HasKeyword(XPD_ObjMod_RepelEnemies_AttackingEnemy_Keyword)
EndFunction

Function ReconcileLocalSecureAreaEnemies()
	Int enemyIndex = 0
	While EnemiesInSecureArea_RefCollAlias != None && enemyIndex < EnemiesInSecureArea_RefCollAlias.GetCount()
		Actor enemyRef = EnemiesInSecureArea_RefCollAlias.GetActorAt(enemyIndex)
		If enemyRef == None || enemyRef.IsDead()
			EnemiesInSecureArea_RefCollAlias.RemoveRef(enemyRef)
		Else
			RegisterForRemoteEvent(enemyRef, "OnDeath")
			enemyIndex += 1
		EndIf
	EndWhile
EndFunction

Function ApplyLocalRepelThresholds(Float oldProgressPercent, Float newProgressPercent)
	Int thresholdIndex = 0
	While ProgressTresholds != None && thresholdIndex < ProgressTresholds.Length
		ProgressTresholdData thresholdData = ProgressTresholds[thresholdIndex]
		If oldProgressPercent < thresholdData.ProgressThreshold_Percent && newProgressPercent >= thresholdData.ProgressThreshold_Percent
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

Function CompleteLocalSecureArea()
	If SecureComplete || IsStageDone(CompleteStage)
		Return
	EndIf
	SecureComplete = True
	CancelTimer(PROGRESS_TIMER)
	SetObjectiveCompleted(SecureArea_Obj, True)
	If XPD_ObjMod_RepelEnemies_AreaSecured_Message != None
		XPD_ObjMod_RepelEnemies_AreaSecured_Message.Show()
	EndIf
	SetStage(CompleteStage)
EndFunction
