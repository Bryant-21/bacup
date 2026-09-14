Event OnQuestInit()
	EnsureLocalModuleLocation()
	InitializeLocalPrisonStages()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If ModuleExpeditionTeam_RefCollAlias != None && ModuleExpeditionTeam_RefCollAlias.Find(playerRef) < 0
			ModuleExpeditionTeam_RefCollAlias.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	ResetDoorKeyInventoryListener(playerRef)
	ReconcileLocalPrisonState()
EndEvent

Function ResetDoorKeyInventoryListener(Actor playerRef)
	RemoveAllInventoryEventFilters()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnItemAdded")
		If DoorKey != None
			AddInventoryEventFilter(DoorKey)
			RegisterForRemoteEvent(playerRef, "OnItemAdded")
		EndIf
	EndIf
EndFunction

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
	RemoveAllInventoryEventFilters()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer() && !IsStageDone(CompleteStage)
		ResetDoorKeyInventoryListener(akSender)
		ReconcileLocalPrisonState()
	EndIf
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If akSender == Game.GetPlayer() && akBaseItem == DoorKey
		ReconcileLocalPrisonState()
	EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
	If akSender == JailorEnemy_Alias.GetReference()
		ReconcileLocalPrisonState()
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	If akActivator == Game.GetPlayer() && PrisonDoors_RefColAlias != None && PrisonDoors_RefColAlias.Find(akSender) >= 0
		Utility.Wait(0.1)
		ReconcileLocalPrisonDoors()
	EndIf
EndEvent

Event ObjectReference.OnLockStateChanged(ObjectReference akSender)
	If PrisonDoors_RefColAlias != None && PrisonDoors_RefColAlias.Find(akSender) >= 0
		ReconcileLocalPrisonDoors()
	EndIf
EndEvent

Function InitializeLocalPrisonStages()
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

Function ReconcileLocalPrisonState()
	If IsStageDone(CompleteStage)
		Return
	EndIf

	JailorEnemy = JailorEnemy_Alias.GetReference() as Actor
	If JailorEnemy != None
		If !IsStageDone(JailorFoundStage)
			EWS_Script = (Self as Quest) as defaultquestencounterwavescript
			If EWS_Script != None && JailorWaveIndex >= 0
				EWS_Script.StartLocalEncounterWave(JailorWaveIndex)
			EndIf
			SetStage(JailorFoundStage)
		EndIf
		SetObjectiveCompleted(300, True)
		SetObjectiveDisplayed(400, True)
		If JailorEnemy.IsDead()
			If !IsStageDone(JailorDeadStage)
				SetStage(JailorDeadStage)
			EndIf
			SetObjectiveCompleted(400, True)
			SetObjectiveDisplayed(500, True)
		Else
			Actor playerRef = Game.GetPlayer()
			If DoorKey != None && JailorEnemy.GetItemCount(DoorKey) == 0 && (playerRef == None || playerRef.GetItemCount(DoorKey) == 0)
				JailorEnemy.AddItem(DoorKey, 1, True)
			EndIf
			RegisterForRemoteEvent(JailorEnemy, "OnDeath")
		EndIf
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef != None && DoorKey != None && playerRef.GetItemCount(DoorKey) > 0
		If !IsStageDone(PlayersHaveKeyStage)
			SetStage(PlayersHaveKeyStage)
		EndIf
		SetObjectiveCompleted(500, True)
		SetObjectiveDisplayed(600, True)
	EndIf
	ReconcileLocalPrisonDoors()
EndFunction

Function ReconcileLocalPrisonDoors()
	If IsStageDone(CompleteStage) || PrisonDoors_RefColAlias == None
		Return
	EndIf

	DoorsToUnlockTotal = PrisonDoors_RefColAlias.GetCount()
	If NumRandomPrisons > 0 && NumRandomPrisons < DoorsToUnlockTotal
		DoorsToUnlockTotal = NumRandomPrisons
	EndIf
	DoorsUnlocked = 0
	Int doorIndex = 0
	While doorIndex < DoorsToUnlockTotal
		ObjectReference prisonDoor = PrisonDoors_RefColAlias.GetAt(doorIndex)
		If prisonDoor != None
			RegisterForRemoteEvent(prisonDoor, "OnActivate")
			RegisterForRemoteEvent(prisonDoor, "OnLockStateChanged")
			If !prisonDoor.IsLocked()
				DoorsUnlocked += 1
				FreeLocalPrisoner(prisonDoor)
			EndIf
		EndIf
		doorIndex += 1
	EndWhile

	If DoorsToUnlockTotal > 0 && DoorsUnlocked >= DoorsToUnlockTotal
		SetObjectiveCompleted(600, True)
		SetStage(CompleteStage)
	EndIf
EndFunction

Function FreeLocalPrisoner(ObjectReference prisonDoor)
	Actor prisonerRef = prisonDoor.GetLinkedRef(XPD_ObjMod_FreePrisoners_LinkPrisoner_Keyword) as Actor
	If prisonerRef != None
		If CaptiveFaction != None
			prisonerRef.RemoveFromFaction(CaptiveFaction)
		EndIf
		prisonerRef.EvaluatePackage()
	EndIf
	ObjectReference prisonFurniture = prisonDoor.GetLinkedRef(XPD_ObjMod_FreePrisoners_LinkPrisonFurniture_Keyword)
	If prisonFurniture != None
		prisonFurniture.DisableNoWait()
	EndIf
EndFunction
