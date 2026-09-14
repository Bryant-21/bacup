Event OnQuestInit()
	InitializeLocalPitt02Mission()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer()
		InitializeLocalPitt02Mission()
	EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
	If akSender == Alias_Actor_HelplessSurvivor_01_Feeble.GetActorReference()
		SetLocalPitt02Stage(5015)
		SetLocalPitt02Stage(5018)
	ElseIf akSender == Alias_Actor_HelplessSurvivor_02_Middling.GetActorReference()
		SetLocalPitt02Stage(5025)
		SetLocalPitt02Stage(5028)
	ElseIf akSender == Alias_Actor_HelplessSurvivor_03_Resilient.GetActorReference()
		SetLocalPitt02Stage(5035)
		SetLocalPitt02Stage(5038)
	EndIf
	ReconcileLocalSurvivors()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID >= 11 && auiStageID <= 13
		RegisterLocalPitt02Selection(auiStageID, True)
	ElseIf auiStageID >= 21 && auiStageID <= 23
		RegisterLocalPitt02Selection(auiStageID, True)
	ElseIf auiStageID >= 31 && auiStageID <= 33
		RegisterLocalPitt02Selection(auiStageID, True)
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
	ElseIf auiStageID == 5005
		RegisterLocalSurvivors()
		StartTimer(1.0, 94)
	ElseIf auiStageID == 5010 || auiStageID == 5015 || auiStageID == 5018 || auiStageID == 5020 || auiStageID == 5025 || auiStageID == 5028 || auiStageID == 5030 || auiStageID == 5035 || auiStageID == 5038
		ReconcileLocalSurvivors()
	ElseIf auiStageID == 400
		SetLocalPitt02Stage(450)
	ElseIf auiStageID == 450
		SetLocalPitt02Stage(460)
	ElseIf auiStageID == 460
		SetLocalPitt02Stage(500)
	ElseIf auiStageID == 500
		SetLocalPitt02Stage(600)
	ElseIf auiStageID == 5200
		SetLocalPitt02Stage(5400)
	ElseIf auiStageID == 5400
		SetLocalPitt02Stage(5600)
	ElseIf auiStageID == 5600
		SetLocalPitt02Stage(9000)
	ElseIf auiStageID == 9000
		ScheduleLocalPitt02Shutdown()
	ElseIf auiStageID == 10000
		CleanupLocalPitt02Mission()
		Stop()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 91
		PollLocalPitt02Phase(1, 1600, 91)
	ElseIf aiTimerID == 92
		PollLocalPitt02Phase(2, 2600, 92)
	ElseIf aiTimerID == 93
		PollLocalPitt02Phase(3, 3600, 93)
	ElseIf aiTimerID == 94
		ReconcileLocalSurvivors()
	ElseIf aiTimerID == 99
		If IsStageDone(9000) && !IsStageDone(10000)
			SetLocalPitt02Stage(10000)
		EndIf
	EndIf
EndEvent

Event OnQuestShutdown()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	CleanupLocalPitt02Mission()
EndEvent

Function InitializeLocalPitt02Mission()
	If IsStageDone(9000)
		If !IsStageDone(10000)
			ScheduleLocalPitt02Shutdown()
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

	Expeditions:Master missionMaster = GetLocalPitt02Master()
	If missionMaster != None
		missionMaster.InitializeSinglePlayerAliases()
	EndIf
	RestoreLocalPitt02Selections()
	If missionMaster != None
		missionMaster.ResumeLocalMission()
	EndIf
	ResumeLocalPitt02PhaseMonitor()
	If IsStageDone(5005) && !IsStageDone(5090)
		RegisterLocalSurvivors()
		StartTimer(1.0, 94)
	EndIf
	If !IsStageDone(400) && !IsStageDone(9000)
		SetStage(400)
	EndIf
EndFunction

Function ScheduleLocalPitt02Shutdown()
	CancelTimer(99)
	StartTimer(2.0, 99)
EndFunction

Expeditions:Master Function GetLocalPitt02Master()
	Return (Self as Quest) as Expeditions:Master
EndFunction

Function RestoreLocalPitt02Selections()
	If IsStageDone(11)
		RegisterLocalPitt02Selection(11, False)
	ElseIf IsStageDone(12)
		RegisterLocalPitt02Selection(12, False)
	ElseIf IsStageDone(13)
		RegisterLocalPitt02Selection(13, False)
	EndIf
	If IsStageDone(21)
		RegisterLocalPitt02Selection(21, False)
	ElseIf IsStageDone(22)
		RegisterLocalPitt02Selection(22, False)
	ElseIf IsStageDone(23)
		RegisterLocalPitt02Selection(23, False)
	EndIf
	If IsStageDone(31)
		RegisterLocalPitt02Selection(31, False)
	ElseIf IsStageDone(32)
		RegisterLocalPitt02Selection(32, False)
	ElseIf IsStageDone(33)
		RegisterLocalPitt02Selection(33, False)
	EndIf
EndFunction

Bool Function RegisterLocalPitt02Selection(Int aiSelectionStage, Bool abResetModule)
	Expeditions:Master missionMaster = GetLocalPitt02Master()
	If missionMaster == None
		Return False
	EndIf

	Int phase = 0
	Int moduleFormID = 0
	If aiSelectionStage == 11
		phase = 1
		moduleFormID = 0x0064DD30
	ElseIf aiSelectionStage == 12
		phase = 1
		moduleFormID = 0x0064C2D0
	ElseIf aiSelectionStage == 13
		phase = 1
		moduleFormID = 0x006507B5
	ElseIf aiSelectionStage == 21
		phase = 2
		moduleFormID = 0x0064D26C
	ElseIf aiSelectionStage == 22
		phase = 2
		moduleFormID = 0x0064CD4D
	ElseIf aiSelectionStage == 23
		phase = 2
		moduleFormID = 0x0064BC52
	ElseIf aiSelectionStage == 31
		phase = 3
		moduleFormID = 0x0064BC50
	ElseIf aiSelectionStage == 32
		phase = 3
		moduleFormID = 0x0064CD4E
	ElseIf aiSelectionStage == 33
		phase = 3
		moduleFormID = 0x0064EA14
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

Function ResumeLocalPitt02PhaseMonitor()
	If IsStageDone(3000) && !IsStageDone(3600)
		StartTimer(1.0, 93)
	ElseIf IsStageDone(2000) && !IsStageDone(2600)
		StartTimer(1.0, 92)
	ElseIf IsStageDone(1000) && !IsStageDone(1600)
		StartTimer(1.0, 91)
	EndIf
EndFunction

Function PollLocalPitt02Phase(Int aiPhase, Int aiCompletionStage, Int aiTimerID)
	If IsStageDone(aiCompletionStage) || IsStageDone(9000)
		Return
	EndIf
	Expeditions:Master missionMaster = GetLocalPitt02Master()
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

Function RegisterLocalSurvivors()
	Int index = 0
	While Alias_Actors_HelplessSurvivors != None && index < Alias_Actors_HelplessSurvivors.GetCount()
		Actor survivorRef = Alias_Actors_HelplessSurvivors.GetActorAt(index)
		If survivorRef != None
			survivorRef.EnableNoWait()
			RegisterForRemoteEvent(survivorRef, "OnDeath")
		EndIf
		index += 1
	EndWhile
EndFunction

Function ReconcileLocalSurvivors()
	Actor firstSurvivor = Alias_Actor_HelplessSurvivor_01_Feeble.GetActorReference()
	Actor secondSurvivor = Alias_Actor_HelplessSurvivor_02_Middling.GetActorReference()
	Actor thirdSurvivor = Alias_Actor_HelplessSurvivor_03_Resilient.GetActorReference()
	If firstSurvivor != None && firstSurvivor.IsDead()
		SetLocalPitt02Stage(5015)
		SetLocalPitt02Stage(5018)
	EndIf
	If secondSurvivor != None && secondSurvivor.IsDead()
		SetLocalPitt02Stage(5025)
		SetLocalPitt02Stage(5028)
	EndIf
	If thirdSurvivor != None && thirdSurvivor.IsDead()
		SetLocalPitt02Stage(5035)
		SetLocalPitt02Stage(5038)
	EndIf

	Bool firstDone = IsStageDone(5010) || IsStageDone(5015)
	Bool secondDone = IsStageDone(5020) || IsStageDone(5025)
	Bool thirdDone = IsStageDone(5030) || IsStageDone(5035)
	If firstDone && secondDone && thirdDone
		SetLocalPitt02Stage(5090)
		If IsStageDone(5010) && IsStageDone(5020) && IsStageDone(5030)
			SetLocalPitt02Stage(5100)
		Else
			SetLocalPitt02Stage(5110)
		EndIf
		CancelTimer(94)
	ElseIf IsStageDone(5005)
		StartTimer(1.0, 94)
	EndIf
EndFunction

Function SetLocalPitt02Stage(Int aiStage)
	If !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction

Function CleanupLocalPitt02Mission()
	CancelTimer(99)
	CancelTimer(91)
	CancelTimer(92)
	CancelTimer(93)
	CancelTimer(94)
	UnregisterForAllEvents()
	Expeditions:Master missionMaster = GetLocalPitt02Master()
	If missionMaster != None
		missionMaster.StopAllObjectiveModules()
	EndIf
EndFunction
