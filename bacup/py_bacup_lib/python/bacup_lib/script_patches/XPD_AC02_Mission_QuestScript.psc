Event OnQuestInit()
	InitializeLocalTaxMission()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer()
		InitializeLocalTaxMission()
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID >= 11 && auiStageID <= 13
		RegisterLocalTaxSelection(auiStageID, True)
	ElseIf auiStageID >= 21 && auiStageID <= 23
		RegisterLocalTaxSelection(auiStageID, True)
	ElseIf auiStageID >= 31 && auiStageID <= 33
		RegisterLocalTaxSelection(auiStageID, True)
	ElseIf auiStageID == 240
		EnsureLocalTaxDialogueQuests()
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
	ElseIf auiStageID >= slotStagesMin && auiStageID <= slotStagesMax
		RecountLocalSlots()
	ElseIf auiStageID == 400
		SetLocalTaxStage(450)
	ElseIf auiStageID == 450
		SetLocalTaxStage(460)
	ElseIf auiStageID == 460
		SetLocalTaxStage(500)
	ElseIf auiStageID == 500
		SetLocalTaxStage(600)
	ElseIf auiStageID == 5200
		SetLocalTaxStage(5400)
	ElseIf auiStageID == 5400
		SetLocalTaxStage(5600)
	ElseIf auiStageID == 5600
		SetLocalTaxStage(9000)
	ElseIf auiStageID == 9000
		ScheduleLocalTaxShutdown()
	ElseIf auiStageID == 10000
		CleanupLocalTaxMission()
		Stop()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 91
		PollLocalTaxPhase(1, 1600, 91)
	ElseIf aiTimerID == 92
		PollLocalTaxPhase(2, 2600, 92)
	ElseIf aiTimerID == 93
		PollLocalTaxPhase(3, 3600, 93)
	ElseIf aiTimerID == 99
		If IsStageDone(9000) && !IsStageDone(10000)
			SetLocalTaxStage(10000)
		EndIf
	EndIf
EndEvent

Event OnQuestShutdown()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	CleanupLocalTaxMission()
EndEvent

Function InitializeLocalTaxMission()
	If IsStageDone(9000)
		If !IsStageDone(10000)
			ScheduleLocalTaxShutdown()
		EndIf
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If PlayerAlias != None && PlayerAlias.GetReference() != playerRef
			PlayerAlias.ForceRefTo(playerRef)
		EndIf
		If Alias_ExpeditionTeam != None && Alias_ExpeditionTeam.Find(playerRef) < 0
			Alias_ExpeditionTeam.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf

	Expeditions:Master missionMaster = GetLocalTaxMaster()
	If missionMaster != None
		missionMaster.InitializeSinglePlayerAliases()
	EndIf
	RestoreLocalTaxSelections()
	RecountLocalSlots()
	If missionMaster != None
		missionMaster.ResumeLocalMission()
	EndIf
	ResumeLocalTaxPhaseMonitor()
	EnsureLocalTaxDialogueQuests()
	If !IsStageDone(400) && !IsStageDone(9000)
		SetStage(400)
	EndIf
EndFunction

Function ScheduleLocalTaxShutdown()
	CancelTimer(99)
	StartTimer(2.0, 99)
EndFunction

Expeditions:Master Function GetLocalTaxMaster()
	Return (Self as Quest) as Expeditions:Master
EndFunction

Function RestoreLocalTaxSelections()
	If IsStageDone(11)
		RegisterLocalTaxSelection(11, False)
	ElseIf IsStageDone(12)
		RegisterLocalTaxSelection(12, False)
	ElseIf IsStageDone(13)
		RegisterLocalTaxSelection(13, False)
	EndIf
	If IsStageDone(21)
		RegisterLocalTaxSelection(21, False)
	ElseIf IsStageDone(22)
		RegisterLocalTaxSelection(22, False)
	ElseIf IsStageDone(23)
		RegisterLocalTaxSelection(23, False)
	EndIf
	If IsStageDone(31)
		RegisterLocalTaxSelection(31, False)
	ElseIf IsStageDone(32)
		RegisterLocalTaxSelection(32, False)
	ElseIf IsStageDone(33)
		RegisterLocalTaxSelection(33, False)
	EndIf
EndFunction

Bool Function RegisterLocalTaxSelection(Int aiSelectionStage, Bool abResetModule)
	Expeditions:Master missionMaster = GetLocalTaxMaster()
	If missionMaster == None
		Return False
	EndIf

	Int phase = 0
	Int moduleFormID = 0
	If aiSelectionStage == 11
		phase = 1
		moduleFormID = 0x0064CD4E
	ElseIf aiSelectionStage == 12
		phase = 1
		moduleFormID = 0x0064CD4D
	ElseIf aiSelectionStage == 13
		phase = 1
		moduleFormID = 0x0064BC52
	ElseIf aiSelectionStage == 21
		phase = 2
		moduleFormID = 0x0064DD30
	ElseIf aiSelectionStage == 22
		phase = 2
		moduleFormID = 0x0064BC50
	ElseIf aiSelectionStage == 23
		phase = 2
		moduleFormID = 0x006507B5
	ElseIf aiSelectionStage >= 31 && aiSelectionStage <= 33
		phase = 3
		moduleFormID = 0x0064BC50
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

Function ResumeLocalTaxPhaseMonitor()
	If IsStageDone(3000) && !IsStageDone(3600)
		StartTimer(1.0, 93)
	ElseIf IsStageDone(2000) && !IsStageDone(2600)
		StartTimer(1.0, 92)
	ElseIf IsStageDone(1000) && !IsStageDone(1600)
		StartTimer(1.0, 91)
	EndIf
EndFunction

Function PollLocalTaxPhase(Int aiPhase, Int aiCompletionStage, Int aiTimerID)
	If IsStageDone(aiCompletionStage) || IsStageDone(9000)
		Return
	EndIf
	Expeditions:Master missionMaster = GetLocalTaxMaster()
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

Function RecountLocalSlots()
	slotCount = 0
	Int stageIndex = slotStagesMin
	While stageIndex <= slotStagesMax
		If IsStageDone(stageIndex)
			slotCount += 1
		EndIf
		stageIndex += 1
	EndWhile
	If slotCount >= TotalSlots && !IsStageDone(slotCompletionStage)
		SetStage(slotCompletionStage)
	EndIf
EndFunction

Function SetLocalTaxStage(Int aiStage)
	If !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction

Function CleanupLocalTaxMission()
	CancelTimer(99)
	CancelTimer(91)
	CancelTimer(92)
	CancelTimer(93)
	StopLocalTaxDialogueQuests()
	Expeditions:Master missionMaster = GetLocalTaxMaster()
	If missionMaster != None
		missionMaster.StopAllObjectiveModules()
	EndIf
EndFunction

Function EnsureLocalTaxDialogueQuests()
	If !IsStageDone(240) || IsStageDone(9000)
		Return
	EndIf
	Quest billyDialogue = Game.GetFormFromFile(0x006D5082, "SeventySix.esm") as Quest
	Quest salDialogue = Game.GetFormFromFile(0x006C3517, "SeventySix.esm") as Quest
	If billyDialogue != None && !billyDialogue.IsRunning() && !billyDialogue.IsStarting()
		If billyDialogue.IsCompleted() || billyDialogue.GetCurrentStageID() > 0
			billyDialogue.Reset()
		EndIf
		billyDialogue.Start()
	EndIf
	If salDialogue != None && !salDialogue.IsRunning() && !salDialogue.IsStarting()
		If salDialogue.IsCompleted() || salDialogue.GetCurrentStageID() > 0
			salDialogue.Reset()
		EndIf
		salDialogue.Start()
	EndIf
EndFunction

Function StopLocalTaxDialogueQuests()
	Quest billyDialogue = Game.GetFormFromFile(0x006D5082, "SeventySix.esm") as Quest
	Quest salDialogue = Game.GetFormFromFile(0x006C3517, "SeventySix.esm") as Quest
	If billyDialogue != None && (billyDialogue.IsRunning() || billyDialogue.IsStarting())
		billyDialogue.Stop()
	EndIf
	If salDialogue != None && (salDialogue.IsRunning() || salDialogue.IsStarting())
		salDialogue.Stop()
	EndIf
EndFunction
