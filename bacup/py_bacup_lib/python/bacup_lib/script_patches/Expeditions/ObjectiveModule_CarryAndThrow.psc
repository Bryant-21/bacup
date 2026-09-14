Event OnQuestInit()
	EnsureLocalModuleLocation()
	InitializeLocalCarryStages()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If ModuleExpeditionTeam_RefCollAlias != None && ModuleExpeditionTeam_RefCollAlias.Find(playerRef) < 0
			ModuleExpeditionTeam_RefCollAlias.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	SuccessfulThrownObjects = new ObjectReference[0]
	PrepareLocalCarryTargets()
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
	SuccessfulThrownObjects.Clear()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If CarryEffect_Spell != None
			playerRef.RemoveSpell(CarryEffect_Spell)
		EndIf
		If CarryObject_Weapon != None && playerRef.GetItemCount(CarryObject_Weapon) > 0
			playerRef.RemoveItem(CarryObject_Weapon, -1, True)
		EndIf
	EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer() && !IsStageDone(CompleteStage)
		PrepareLocalCarryTargets()
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	If akActivator == Game.GetPlayer() && ObjectSources_RefCollAlias != None && ObjectSources_RefCollAlias.Find(akSender) >= 0
		EquipLocalCarryObject()
	EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	HandleLocalThrownObject(akSender, akActionRef)
EndEvent

Function InitializeLocalCarryStages()
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
	ModuleActive = True
EndFunction

Function PrepareLocalCarryTargets()
	If IsStageDone(CompleteStage) || ObjectTargetSets == None
		Return
	EndIf
	EWS_Script = (Self as Quest) as defaultquestencounterwavescript
	If ObjectSources_RefCollAlias != None
		ObjectSources_RefCollAlias.EnableAll()
	EndIf
	If CarryObjects_RefCollAlias != None
		CarryObjects_RefCollAlias.EnableAll()
	EndIf
	Int sourceIndex = 0
	While ObjectSources_RefCollAlias != None && sourceIndex < ObjectSources_RefCollAlias.GetCount()
		ObjectReference sourceRef = ObjectSources_RefCollAlias.GetAt(sourceIndex)
		If sourceRef != None
			RegisterForRemoteEvent(sourceRef, "OnActivate")
		EndIf
		sourceIndex += 1
	EndWhile

	TargetPointsCompleted = 0
	Int targetLimit = GetLocalCarryTargetLimit()
	Int targetIndex = 0
	Int selectedTargets = 0
	While targetIndex < ObjectTargetSets.Length && selectedTargets < targetLimit
		ObjectTargetData targetData = ObjectTargetSets[targetIndex]
		If IsLocalCarryTargetValid(targetIndex)
			selectedTargets += 1
			If targetData.StageToSetOnCompleted >= 0 && IsStageDone(targetData.StageToSetOnCompleted)
				TargetPointsCompleted += 1
			Else
				If targetData.StageToSetOnTargetActive >= 0 && !IsStageDone(targetData.StageToSetOnTargetActive)
					SetStage(targetData.StageToSetOnTargetActive)
				EndIf
				If targetData.StageToSetOnTargetActive >= 0
					SetObjectiveDisplayed(targetData.StageToSetOnTargetActive, True)
				EndIf
				RegisterLocalCarryTarget(targetData)
			EndIf
		EndIf
		targetIndex += 1
	EndWhile
	If targetLimit > 0 && TargetPointsCompleted >= targetLimit
		CompleteLocalCarryModule()
	EndIf
EndFunction

Function RegisterLocalCarryTarget(ObjectTargetData targetData)
	If targetData.TargetTriggers_RefCollAlias != None
		targetData.TargetTriggers_RefCollAlias.EnableAll()
		Int triggerIndex = 0
		While triggerIndex < targetData.TargetTriggers_RefCollAlias.GetCount()
			ObjectReference triggerRef = targetData.TargetTriggers_RefCollAlias.GetAt(triggerIndex)
			If triggerRef != None
				RegisterForRemoteEvent(triggerRef, "OnTriggerEnter")
			EndIf
			triggerIndex += 1
		EndWhile
	EndIf
	If targetData.TargetDestrucibles_RefColAlias != None
		targetData.TargetDestrucibles_RefColAlias.EnableAll()
	EndIf
EndFunction

Function EquipLocalCarryObject()
	If !ModuleActive || IsStageDone(CompleteStage)
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || CarryObject_Weapon == None
		Return
	EndIf
	If playerRef.GetItemCount(CarryObject_Weapon) == 0
		playerRef.AddItem(CarryObject_Weapon, 1, True)
	EndIf
	playerRef.EquipItem(CarryObject_Weapon, False, True)
	If CarryEffect_Spell != None
		playerRef.AddSpell(CarryEffect_Spell, False)
	EndIf
	If CarryObject_CollectedAndEquipped_Message != None
		CarryObject_CollectedAndEquipped_Message.Show()
	EndIf
EndFunction

Function HandleLocalThrownObject(ObjectReference targetTrigger, ObjectReference thrownRef)
	If !ModuleActive || IsStageDone(CompleteStage) || targetTrigger == None || thrownRef == None || thrownRef == Game.GetPlayer()
		Return
	EndIf
	Bool isCarryObject = CarryObjects_RefCollAlias != None && CarryObjects_RefCollAlias.Find(thrownRef) >= 0
	If !isCarryObject && thrownRef.GetBaseObject() != CarryObject_Weapon
		Return
	EndIf
	If SuccessfulThrownObjects.Find(thrownRef) >= 0
		Return
	EndIf

	Int targetLimit = GetLocalCarryTargetLimit()
	Int targetIndex = 0
	Int selectedTargets = 0
	While targetIndex < ObjectTargetSets.Length && selectedTargets < targetLimit
		ObjectTargetData targetData = ObjectTargetSets[targetIndex]
		If IsLocalCarryTargetValid(targetIndex)
			selectedTargets += 1
		EndIf
		If IsLocalCarryTargetValid(targetIndex) && targetData.TargetTriggers_RefCollAlias.Find(targetTrigger) >= 0
			If targetData.StageToSetOnCompleted >= 0 && IsStageDone(targetData.StageToSetOnCompleted)
				Return
			EndIf
			SuccessfulThrownObjects.Add(thrownRef)
			targetData.SuccessfulHits += 1
			Int requiredHits = targetData.RequiredHits
			If requiredHits <= 0
				requiredHits = 1
			EndIf
			If targetData.SuccessfulHits >= requiredHits
				CompleteLocalCarryTarget(targetIndex)
			EndIf
			Return
		EndIf
		targetIndex += 1
	EndWhile
EndFunction

Function CompleteLocalCarryTarget(Int targetIndex)
	ObjectTargetData targetData = ObjectTargetSets[targetIndex]
	If targetData.StageToSetOnCompleted >= 0 && IsStageDone(targetData.StageToSetOnCompleted)
		Return
	EndIf
	If targetData.StageToSetOnCompleted >= 0
		SetStage(targetData.StageToSetOnCompleted)
	EndIf
	If targetData.StageToSetOnTargetActive >= 0
		SetObjectiveCompleted(targetData.StageToSetOnTargetActive, True)
	EndIf
	TargetPointsCompleted += 1
	If UseReinforcingWaves && EWS_Script != None && targetData.EWS_ReinforcementsWave >= 0
		EWS_Script.StartLocalEncounterWave(targetData.EWS_ReinforcementsWave)
	EndIf
	If TargetPointsCompleted >= GetLocalCarryTargetLimit()
		CompleteLocalCarryModule()
	EndIf
EndFunction

Function CompleteLocalCarryModule()
	If IsStageDone(CompleteStage)
		Return
	EndIf
	ModuleActive = False
	SetObjectiveCompleted(300, True)
	SetStage(CompleteStage)
EndFunction

Int Function GetLocalCarryTargetLimit()
	Int validCount = 0
	Int targetIndex = 0
	While ObjectTargetSets != None && targetIndex < ObjectTargetSets.Length
		If IsLocalCarryTargetValid(targetIndex)
			validCount += 1
		EndIf
		targetIndex += 1
	EndWhile
	If NumTargetsToHit > 0 && NumTargetsToHit < validCount
		validCount = NumTargetsToHit
	EndIf
	Return validCount
EndFunction

Bool Function IsLocalCarryTargetValid(Int targetIndex)
	If ObjectTargetSets == None || targetIndex < 0 || targetIndex >= ObjectTargetSets.Length
		Return False
	EndIf
	RefCollectionAlias targetTriggers = ObjectTargetSets[targetIndex].TargetTriggers_RefCollAlias
	Return targetTriggers != None && targetTriggers.GetCount() > 0
EndFunction
