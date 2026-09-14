Event OnQuestInit()
	InitializeLocalSensationMission()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer()
		InitializeLocalSensationMission()
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID >= 11 && auiStageID <= 13
		RegisterLocalSensationSelection(auiStageID, True)
	ElseIf auiStageID >= 21 && auiStageID <= 23
		RegisterLocalSensationSelection(auiStageID, True)
	ElseIf auiStageID >= 31 && auiStageID <= 33
		RegisterLocalSensationSelection(auiStageID, True)
	ElseIf auiStageID == 300
		EnsureLocalSensationDialogueQuests()
	ElseIf auiStageID == 1000
		StartTimer(1.0, 91)
	ElseIf auiStageID == 1600
		CancelTimer(91)
	ElseIf auiStageID == 2000
		StartTimer(1.0, 92)
	ElseIf auiStageID == 2600
		CancelTimer(92)
	ElseIf auiStageID == 3000
		StartTimer(1.0, 93)
	ElseIf auiStageID == 3600
		CancelTimer(93)
	ElseIf auiStageID == 400
		SetLocalSensationStage(450)
	ElseIf auiStageID == 450
		SetLocalSensationStage(460)
	ElseIf auiStageID == 460
		SetLocalSensationStage(500)
	ElseIf auiStageID == 500
		SetLocalSensationStage(600)
	ElseIf auiStageID == 5200
		SetLocalSensationStage(5400)
	ElseIf auiStageID == 5400
		SetLocalSensationStage(5600)
	ElseIf auiStageID == 5600
		SetLocalSensationStage(9000)
	ElseIf auiStageID == 9000
		ScheduleLocalSensationShutdown()
	ElseIf auiStageID == 10000
		CleanupLocalSensationMission()
		Stop()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 91
		PollLocalSensationPhase(1, 1600, 91)
	ElseIf aiTimerID == 92
		PollLocalSensationPhase(2, 2600, 92)
	ElseIf aiTimerID == 93
		PollLocalSensationPhase(3, 3600, 93)
	ElseIf aiTimerID == 99
		If IsStageDone(9000) && !IsStageDone(10000)
			SetLocalSensationStage(10000)
		EndIf
	EndIf
EndEvent

Event OnQuestShutdown()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	CleanupLocalSensationMission()
EndEvent

Function InitializeLocalSensationMission()
	If IsStageDone(9000)
		If !IsStageDone(10000)
			ScheduleLocalSensationShutdown()
		EndIf
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	XPDLeader = playerRef
	If playerRef != None
		If Alias_ExpeditionLeader != None && Alias_ExpeditionLeader.GetReference() != playerRef
			Alias_ExpeditionLeader.ForceRefTo(playerRef)
		EndIf
		If Alias_ExpeditionTeam != None && Alias_ExpeditionTeam.Find(playerRef) < 0
			Alias_ExpeditionTeam.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf

	Expeditions:Master missionMaster = GetLocalSensationMaster()
	If missionMaster != None
		missionMaster.InitializeSinglePlayerAliases()
	EndIf
	RestoreLocalSensationSelections()
	If missionMaster != None
		missionMaster.ResumeLocalMission()
	EndIf
	ResumeLocalSensationPhaseMonitor()
	EnsureLocalSensationDialogueQuests()
	If !IsStageDone(400) && !IsStageDone(9000)
		SetStage(400)
	EndIf
EndFunction

Function ScheduleLocalSensationShutdown()
	CancelTimer(99)
	StartTimer(2.0, 99)
EndFunction

Expeditions:Master Function GetLocalSensationMaster()
	Return (Self as Quest) as Expeditions:Master
EndFunction

Function RestoreLocalSensationSelections()
	If IsStageDone(11)
		RegisterLocalSensationSelection(11, False)
	ElseIf IsStageDone(12)
		RegisterLocalSensationSelection(12, False)
	ElseIf IsStageDone(13)
		RegisterLocalSensationSelection(13, False)
	EndIf
	If IsStageDone(21)
		RegisterLocalSensationSelection(21, False)
	ElseIf IsStageDone(22)
		RegisterLocalSensationSelection(22, False)
	ElseIf IsStageDone(23)
		RegisterLocalSensationSelection(23, False)
	EndIf
	If IsStageDone(31)
		RegisterLocalSensationSelection(31, False)
	ElseIf IsStageDone(32)
		RegisterLocalSensationSelection(32, False)
	ElseIf IsStageDone(33)
		RegisterLocalSensationSelection(33, False)
	EndIf
EndFunction

Bool Function RegisterLocalSensationSelection(Int aiSelectionStage, Bool abResetModule)
	Expeditions:Master missionMaster = GetLocalSensationMaster()
	If missionMaster == None
		Return False
	EndIf

	Int phase = 0
	Int moduleFormID = 0
	If aiSelectionStage == 11
		phase = 1
		moduleFormID = 0x0064BC52
	ElseIf aiSelectionStage == 12
		phase = 1
		moduleFormID = 0x0064D26C
	ElseIf aiSelectionStage == 13
		phase = 1
		moduleFormID = 0x006BABB6
	ElseIf aiSelectionStage == 21
		phase = 2
		moduleFormID = 0x006507B5
	ElseIf aiSelectionStage == 22
		phase = 2
		moduleFormID = 0x0064DD30
	ElseIf aiSelectionStage == 23
		phase = 2
		moduleFormID = 0x0064BC50
	ElseIf aiSelectionStage == 31
		phase = 3
		moduleFormID = 0x0064CD4E
	ElseIf aiSelectionStage == 32
		phase = 3
		moduleFormID = 0x0064EA14
	ElseIf aiSelectionStage == 33
		phase = 3
		moduleFormID = 0x0064CD4D
	EndIf
	If phase == 0
		Return False
	EndIf

	Quest moduleQuest = Game.GetFormFromFile(moduleFormID, "SeventySix.esm") as Quest
	Bool registered = missionMaster.RegisterObjectiveModule(phase, moduleQuest, abResetModule)
	If registered && ((phase == 1 && IsStageDone(1000) && !IsStageDone(1600)) || (phase == 2 && IsStageDone(2000) && !IsStageDone(2600)) || (phase == 3 && IsStageDone(3000) && !IsStageDone(3600)))
		missionMaster.StartObjectiveModuleForPhase(phase)
	EndIf
	Return registered
EndFunction

Function ResumeLocalSensationPhaseMonitor()
	If IsStageDone(3000) && !IsStageDone(3600)
		StartTimer(1.0, 93)
	ElseIf IsStageDone(2000) && !IsStageDone(2600)
		StartTimer(1.0, 92)
	ElseIf IsStageDone(1000) && !IsStageDone(1600)
		StartTimer(1.0, 91)
	EndIf
EndFunction

Function PollLocalSensationPhase(Int aiPhase, Int aiCompletionStage, Int aiTimerID)
	If IsStageDone(aiCompletionStage) || IsStageDone(9000)
		Return
	EndIf
	Expeditions:Master missionMaster = GetLocalSensationMaster()
	Quest moduleQuest = None
	If missionMaster != None
		moduleQuest = missionMaster.GetObjectiveModuleQuest(aiPhase)
	EndIf
	If moduleQuest != None && moduleQuest.IsStageDone(9000)
		SetStage(aiCompletionStage)
	Else
		StartTimer(1.0, aiTimerID)
	EndIf
EndFunction

Function SetLocalSensationStage(Int aiStage)
	If !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction

Function CleanupLocalSensationMission()
	CancelTimer(99)
	CancelTimer(91)
	CancelTimer(92)
	CancelTimer(93)
	StopLocalSensationDialogueQuests()
	Expeditions:Master missionMaster = GetLocalSensationMaster()
	If missionMaster != None
		missionMaster.StopAllObjectiveModules()
	EndIf
EndFunction

Function EnsureLocalSensationDialogueQuests()
	If !IsStageDone(300) || IsStageDone(9000)
		Return
	EndIf
	Quest charlotteDialogue = Game.GetFormFromFile(0x006FA1DF, "SeventySix.esm") as Quest
	Quest veracioDialogue = Game.GetFormFromFile(0x006FAA69, "SeventySix.esm") as Quest
	If charlotteDialogue != None && !charlotteDialogue.IsRunning() && !charlotteDialogue.IsStarting()
		If charlotteDialogue.IsCompleted() || charlotteDialogue.GetCurrentStageID() > 0
			charlotteDialogue.Reset()
		EndIf
		charlotteDialogue.Start()
	EndIf
	If veracioDialogue != None && !veracioDialogue.IsRunning() && !veracioDialogue.IsStarting()
		If veracioDialogue.IsCompleted() || veracioDialogue.GetCurrentStageID() > 0
			veracioDialogue.Reset()
		EndIf
		veracioDialogue.Start()
	EndIf
EndFunction

Function StopLocalSensationDialogueQuests()
	Quest charlotteDialogue = Game.GetFormFromFile(0x006FA1DF, "SeventySix.esm") as Quest
	Quest veracioDialogue = Game.GetFormFromFile(0x006FAA69, "SeventySix.esm") as Quest
	If charlotteDialogue != None && (charlotteDialogue.IsRunning() || charlotteDialogue.IsStarting())
		charlotteDialogue.Stop()
	EndIf
	If veracioDialogue != None && (veracioDialogue.IsRunning() || veracioDialogue.IsStarting())
		veracioDialogue.Stop()
	EndIf
EndFunction
