Event OnQuestInit()
	InitializeLocalHumanMission()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer()
		InitializeLocalHumanMission()
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == 100
		If IsStageDone(500)
			SetLocalHumanStage(600)
		EndIf
	ElseIf auiStageID == 200
		EnsureLocalHumanDialogueQuests()
	ElseIf auiStageID == 400
		SetLocalHumanStage(450)
	ElseIf auiStageID == 450
		SetLocalHumanStage(500)
	ElseIf auiStageID == 500
		; FO76 set arrival when the vertibird touched down; Tales delivers the player by travel instead.
		If IsStageDone(100)
			SetLocalHumanStage(600)
		EndIf
	ElseIf auiStageID == 800
		StartTimer(3.0, 94)
	ElseIf auiStageID == 1000
		StartLocalHumanPhase(1)
	ElseIf auiStageID == 2000
		StartLocalHumanPhase(2)
	ElseIf auiStageID == 3000
		StartLocalHumanPhase(3)
	ElseIf auiStageID == 5200
		SetLocalHumanStage(5400)
	ElseIf auiStageID == 5400
		SetLocalHumanStage(5600)
	ElseIf auiStageID == 5600
		SetLocalHumanStage(9000)
	ElseIf auiStageID == 9000
		ScheduleLocalHumanShutdown()
	ElseIf auiStageID == 10000
		CleanupLocalHumanMission()
		Stop()
	EndIf
	ArmLocalHumanMonitor()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 90
		PollLocalHumanMission()
		ArmLocalHumanMonitor()
	ElseIf aiTimerID == 94
		If IsStageDone(800) && !IsStageDone(1000)
			If IsLocalHumanScenePlaying(0x006DBC34) || IsLocalHumanScenePlaying(0x006EF821)
				StartTimer(1.0, 94)
			Else
				SetLocalHumanStage(1000)
			EndIf
		EndIf
	ElseIf aiTimerID == 95
		If IsStageDone(4700) && !IsStageDone(5200)
			If IsLocalHumanScenePlaying(0x006EF822) || IsLocalHumanScenePlaying(0x006DD95E)
				StartTimer(1.0, 95)
			Else
				SetLocalHumanStage(5200)
			EndIf
		EndIf
	ElseIf aiTimerID == 99
		If IsStageDone(9000) && !IsStageDone(10000)
			SetLocalHumanStage(10000)
		EndIf
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer() || IsStageDone(9000)
		Return
	EndIf
	If akSender == GetLocalHumanAliasRef(114)
		If IsStageDone(600) && !IsStageDone(800)
			SetStage(800)
		ElseIf IsStageDone(1600) && !IsStageDone(1700)
			SetStage(1700)
		EndIf
	ElseIf akSender == GetLocalHumanAliasRef(247)
		If IsStageDone(200) && !IsStageDone(850)
			SetStage(850)
		EndIf
	ElseIf akSender == GetLocalHumanAliasRef(280)
		If IsStageDone(4700) && !IsStageDone(5200)
			StartTimer(3.0, 95)
		EndIf
	ElseIf !IsStageDone(870)
		Int cameraIndex = 0
		While cameraIndex < 5
			If IsStageDone(851 + cameraIndex) && akSender == GetLocalHumanAliasRef(273 + cameraIndex)
				SetStage(870)
			EndIf
			cameraIndex += 1
		EndWhile
	EndIf
EndEvent

Event OnQuestShutdown()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	CleanupLocalHumanMission()
EndEvent

Function InitializeLocalHumanMission()
	If IsStageDone(9000)
		If !IsStageDone(10000)
			ScheduleLocalHumanShutdown()
		EndIf
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If PlayerAlias != None && PlayerAlias.GetReference() != playerRef
			PlayerAlias.ForceRefTo(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	RegisterLocalHumanContacts()

	Expeditions:Master missionMaster = GetLocalHumanMaster()
	If missionMaster != None
		missionMaster.InitializeSinglePlayerAliases()
	EndIf
	If IsStageDone(1000) && !IsStageDone(1600)
		StartLocalHumanPhase(1)
	ElseIf IsStageDone(2000) && !IsStageDone(2600)
		StartLocalHumanPhase(2)
	ElseIf IsStageDone(3000) && !IsStageDone(3600)
		StartLocalHumanPhase(3)
	EndIf
	EnsureLocalHumanDialogueQuests()
	If IsStageDone(800) && !IsStageDone(1000)
		StartTimer(3.0, 94)
	EndIf
	If !IsStageDone(400)
		SetStage(400)
	ElseIf IsStageDone(500) && IsStageDone(100)
		SetLocalHumanStage(600)
	EndIf
	ArmLocalHumanMonitor()
EndFunction

Function RegisterLocalHumanContacts()
	Int[] contactAliases = New Int[8]
	contactAliases[0] = 114
	contactAliases[1] = 247
	contactAliases[2] = 280
	contactAliases[3] = 273
	contactAliases[4] = 274
	contactAliases[5] = 275
	contactAliases[6] = 276
	contactAliases[7] = 277
	Int index = 0
	While index < contactAliases.Length
		ObjectReference contactRef = GetLocalHumanAliasRef(contactAliases[index])
		If contactRef != None
			RegisterForRemoteEvent(contactRef, "OnActivate")
		EndIf
		index += 1
	EndWhile
EndFunction

ObjectReference Function GetLocalHumanAliasRef(Int aiAliasID)
	ReferenceAlias targetAlias = GetAlias(aiAliasID) as ReferenceAlias
	If targetAlias == None
		Return None
	EndIf
	Return targetAlias.GetReference()
EndFunction

Expeditions:Master Function GetLocalHumanMaster()
	Return (Self as Quest) as Expeditions:Master
EndFunction

Function ArmLocalHumanMonitor()
	CancelTimer(90)
	If IsRunning() && IsStageDone(600) && !IsStageDone(9000)
		StartTimer(1.0, 90)
	EndIf
EndFunction

Function ScheduleLocalHumanShutdown()
	CancelTimer(99)
	StartTimer(2.0, 99)
EndFunction

Function SetLocalHumanStage(Int aiStage)
	If !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction

Bool Function IsLocalHumanScenePlaying(Int aiSceneFormID)
	Scene localScene = Game.GetFormFromFile(aiSceneFormID, "SeventySix.esm") as Scene
	Return localScene != None && localScene.IsPlaying()
EndFunction

Int Function GetLocalHumanSelection(Int aiPhase)
	Int firstStage = aiPhase * 10 + 1
	If IsStageDone(firstStage)
		Return firstStage
	ElseIf IsStageDone(firstStage + 1)
		Return firstStage + 1
	ElseIf IsStageDone(firstStage + 2)
		Return firstStage + 2
	EndIf
	Return 0
EndFunction

; QMDL records bound each FO76 objective slot to a generic module quest; FO4 has no QMDL.
Quest Function GetLocalHumanModule(Int aiSelectionStage)
	Int moduleFormID = 0
	If aiSelectionStage == 11
		moduleFormID = 0x0064EA14
	ElseIf aiSelectionStage == 12
		moduleFormID = 0x0064CD4D
	ElseIf aiSelectionStage == 13
		moduleFormID = 0x0064DD30
	ElseIf aiSelectionStage == 21
		moduleFormID = 0x006507B5
	ElseIf aiSelectionStage == 22
		moduleFormID = 0x0064BC50
	ElseIf aiSelectionStage == 23
		moduleFormID = 0x0064D26C
	ElseIf aiSelectionStage == 31
		moduleFormID = 0x0064CD4E
	EndIf
	If moduleFormID == 0
		Return None
	EndIf
	Return Game.GetFormFromFile(moduleFormID, "SeventySix.esm") as Quest
EndFunction

Int Function GetLocalHumanPhaseCompleteStage(Int aiPhase)
	Return aiPhase * 1000 + 600
EndFunction

Location Function GetLocalHumanPhaseLocation(Int aiPhase)
	Int locationAliasID = 93
	If aiPhase == 1
		locationAliasID = 8
	EndIf
	LocationAlias phaseLocation = GetAlias(locationAliasID) as LocationAlias
	If phaseLocation == None
		Return None
	EndIf
	Return phaseLocation.GetLocation()
EndFunction

Function StartLocalHumanPhase(Int aiPhase)
	Int completeStage = GetLocalHumanPhaseCompleteStage(aiPhase)
	If !IsStageDone(aiPhase * 1000) || IsStageDone(completeStage) || IsStageDone(9000)
		Return
	EndIf
	Int selectionStage = GetLocalHumanSelection(aiPhase)
	If selectionStage == 0
		Return
	EndIf
	Quest moduleQuest = GetLocalHumanModule(selectionStage)
	If moduleQuest == None
		; The C02/C03 slots are FO76 stub modules that finish immediately.
		SetStage(completeStage)
		Return
	EndIf

	If !moduleQuest.IsRunning() && !moduleQuest.IsStarting()
		If moduleQuest.IsCompleted() || moduleQuest.GetStage() > 0
			moduleQuest.Reset()
		EndIf
		; Force the module into its phase location before start; the Master fallback would use the player's
		; current location, which is City Hall when Mayor Tim hands out the next phase.
		LocationAlias moduleLocation = moduleQuest.GetAlias(3) as LocationAlias
		Location phaseLocation = GetLocalHumanPhaseLocation(aiPhase)
		If moduleLocation != None && phaseLocation != None
			moduleLocation.ForceLocationTo(phaseLocation)
		EndIf
		If !moduleQuest.Start()
			Debug.Trace("[XPD_AC03] objective module failed to start phase=" + aiPhase as String, 1)
			Return
		EndIf
	EndIf
	Expeditions:Master missionMaster = GetLocalHumanMaster()
	If missionMaster != None && missionMaster.GetObjectiveModuleQuest(aiPhase) != moduleQuest
		missionMaster.RegisterObjectiveModule(aiPhase, moduleQuest, False)
	EndIf
EndFunction

Function PollLocalHumanPhase(Int aiPhase)
	Int completeStage = GetLocalHumanPhaseCompleteStage(aiPhase)
	If !IsStageDone(aiPhase * 1000) || IsStageDone(completeStage)
		Return
	EndIf
	Int selectionStage = GetLocalHumanSelection(aiPhase)
	Quest moduleQuest = GetLocalHumanModule(selectionStage)
	If moduleQuest == None
		StartLocalHumanPhase(aiPhase)
		Return
	EndIf
	If moduleQuest.IsStageDone(9000)
		SetStage(completeStage)
		Return
	EndIf
	If !moduleQuest.IsRunning() && !moduleQuest.IsStarting()
		StartLocalHumanPhase(aiPhase)
		Return
	EndIf
	MirrorLocalHumanModuleStages(selectionStage, moduleQuest)
EndFunction

; The QMDL stage links (QMSI -> parent stage) that the mission's notes and dialogue conditions read.
Function MirrorLocalHumanModuleStages(Int aiSelectionStage, Quest akModuleQuest)
	If aiSelectionStage == 11
		MirrorLocalHumanModuleStage(akModuleQuest, 400, 1010)
	ElseIf aiSelectionStage == 12
		MirrorLocalHumanModuleStage(akModuleQuest, 300, 1020)
		MirrorLocalHumanModuleStage(akModuleQuest, 500, 1030)
		MirrorLocalHumanModuleStage(akModuleQuest, 510, 1035)
		MirrorLocalHumanModuleStage(akModuleQuest, 520, 1040)
	ElseIf aiSelectionStage == 13
		MirrorLocalHumanModuleStage(akModuleQuest, 300, 1025)
	ElseIf aiSelectionStage == 21
		MirrorLocalHumanModuleStage(akModuleQuest, 600, 2030)
		MirrorLocalHumanModuleStage(akModuleQuest, 601, 2035)
		MirrorLocalHumanModuleStage(akModuleQuest, 602, 2040)
	ElseIf aiSelectionStage == 22
		MirrorLocalHumanModuleStage(akModuleQuest, 500, 2010)
		MirrorLocalHumanModuleStage(akModuleQuest, 510, 2015)
		MirrorLocalHumanModuleStage(akModuleQuest, 520, 2020)
	EndIf
EndFunction

Function MirrorLocalHumanModuleStage(Quest akModuleQuest, Int aiModuleStage, Int aiParentStage)
	If akModuleQuest.IsStageDone(aiModuleStage) && !IsStageDone(aiParentStage)
		SetStage(aiParentStage)
	EndIf
EndFunction

Function PollLocalHumanMission()
	If IsStageDone(9000)
		Return
	EndIf
	PollLocalHumanPhase(1)
	PollLocalHumanPhase(2)
	PollLocalHumanPhase(3)
	UpdateLocalHumanDossiers()
	PollLocalHumanBossFight()
EndFunction

Function UpdateLocalHumanDossiers()
	Expeditions:CollectibleItemObjective collectible = (Self as Quest) as Expeditions:CollectibleItemObjective
	Actor playerRef = Game.GetPlayer()
	If collectible == None || playerRef == None || collectible.CollectibleItem == None
		Return
	EndIf
	If !IsStageDone(collectible.Stage_SetUpObjective) || IsStageDone(collectible.Stage_CollectedAllItems)
		Return
	EndIf
	Int collected = playerRef.GetItemCount(collectible.CollectibleItem)
	If collected > collectible.ItemsToCollectTotal
		collected = collectible.ItemsToCollectTotal
	EndIf
	B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable(collectible.TextVar_ItemsCollected, collected as Float)
	EndIf
	If collectible.ItemsCollected_AV != None
		playerRef.SetValue(collectible.ItemsCollected_AV, collected as Float)
	EndIf
	If collected >= collectible.ItemsToCollectTotal
		SetStage(collectible.Stage_CollectedAllItems)
	EndIf
EndFunction

Function PollLocalHumanBossFight()
	If IsStageDone(150) && !IsStageDone(4600)
		Actor playerRef = Game.GetPlayer()
		ObjectReference finaleTrigger = GetLocalHumanAliasRef(258)
		If finaleTrigger == None || (playerRef != None && playerRef.GetDistance(finaleTrigger) < 2048.0)
			SetStage(4600)
		EndIf
	ElseIf IsStageDone(4600) && !IsStageDone(4700)
		Actor boss = GetLocalHumanAliasRef(115) as Actor
		If boss == None || boss.IsDead()
			SetStage(4700)
			Return
		EndIf
		; FO76 released a new pollinator swarm as the Sporemaster weakened.
		Float healthPercent = boss.GetValuePercentage(Game.GetHealthAV())
		If healthPercent <= 0.75
			SetLocalHumanStage(4511)
		EndIf
		If healthPercent <= 0.5
			SetLocalHumanStage(4512)
		EndIf
		If healthPercent <= 0.25
			SetLocalHumanStage(4513)
		EndIf
	EndIf
EndFunction

Function EnsureLocalHumanDialogueQuests()
	If !IsStageDone(200) || IsStageDone(9000)
		Return
	EndIf
	EnsureLocalHumanDialogueQuest(0x006DE1E5)
	EnsureLocalHumanDialogueQuest(0x006E337F)
	EnsureLocalHumanDialogueQuest(0x006E06AC)
EndFunction

Function EnsureLocalHumanDialogueQuest(Int aiQuestFormID)
	Quest dialogueQuest = Game.GetFormFromFile(aiQuestFormID, "SeventySix.esm") as Quest
	If dialogueQuest != None && !dialogueQuest.IsRunning() && !dialogueQuest.IsStarting()
		If dialogueQuest.IsCompleted() || dialogueQuest.GetCurrentStageID() > 0
			dialogueQuest.Reset()
		EndIf
		dialogueQuest.Start()
	EndIf
EndFunction

Function StopLocalHumanDialogueQuest(Int aiQuestFormID)
	Quest dialogueQuest = Game.GetFormFromFile(aiQuestFormID, "SeventySix.esm") as Quest
	If dialogueQuest != None && (dialogueQuest.IsRunning() || dialogueQuest.IsStarting())
		dialogueQuest.Stop()
	EndIf
EndFunction

Function CleanupLocalHumanMission()
	CancelTimer(90)
	CancelTimer(94)
	CancelTimer(95)
	CancelTimer(99)
	StopLocalHumanDialogueQuest(0x006DE1E5)
	StopLocalHumanDialogueQuest(0x006E337F)
	StopLocalHumanDialogueQuest(0x006E06AC)
	Expeditions:Master missionMaster = GetLocalHumanMaster()
	If missionMaster != None
		missionMaster.StopAllObjectiveModules()
	EndIf
	RestoreLocalHumanWorldState()
EndFunction

; These placed references persist across Tales replays, so return them to their authored enable state.
Function RestoreLocalHumanWorldState()
	Int[] finaleAliases = New Int[8]
	finaleAliases[0] = 115
	finaleAliases[1] = 252
	finaleAliases[2] = 253
	finaleAliases[3] = 254
	finaleAliases[4] = 280
	finaleAliases[5] = 282
	finaleAliases[6] = 260
	finaleAliases[7] = 281
	Int index = 0
	While index < finaleAliases.Length
		SetLocalHumanAliasEnabled(finaleAliases[index], False)
		index += 1
	EndWhile
	index = 273
	While index <= 277
		SetLocalHumanAliasEnabled(index, False)
		index += 1
	EndWhile
	SetLocalHumanAliasEnabled(288, True)
EndFunction

Function SetLocalHumanAliasEnabled(Int aiAliasID, Bool abEnabled)
	Alias targetAlias = GetAlias(aiAliasID)
	RefCollectionAlias targetCollection = targetAlias as RefCollectionAlias
	If targetCollection != None
		Int index = 0
		While index < targetCollection.GetCount()
			SetLocalHumanRefEnabled(targetCollection.GetAt(index), abEnabled)
			index += 1
		EndWhile
	Else
		ReferenceAlias targetReference = targetAlias as ReferenceAlias
		If targetReference != None
			SetLocalHumanRefEnabled(targetReference.GetReference(), abEnabled)
		EndIf
	EndIf
EndFunction

Function SetLocalHumanRefEnabled(ObjectReference akRef, Bool abEnabled)
	If akRef == None
		Return
	EndIf
	If abEnabled
		akRef.EnableNoWait()
	Else
		akRef.DisableNoWait()
	EndIf
EndFunction
