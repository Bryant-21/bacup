Event OnQuestInit()
	InitializeLocalPitt01Mission()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer()
		InitializeLocalPitt01Mission()
	EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
	If IsLocalFinaleFighter(akSender) && !IsStageDone(ProtectUnionersFailedStage)
		SetStage(ProtectUnionersFailedStage)
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID >= 11 && auiStageID <= 13
		RegisterLocalPitt01Selection(auiStageID, True)
	ElseIf auiStageID >= 21 && auiStageID <= 23
		RegisterLocalPitt01Selection(auiStageID, True)
	ElseIf auiStageID >= 31 && auiStageID <= 33
		RegisterLocalPitt01Selection(auiStageID, True)
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
	ElseIf auiStageID == FinaleTrogAttackStage
		RegisterLocalFinaleFighters()
		StartTimer(1.0, 94)
	ElseIf auiStageID == FinaleCompleteStage
		ReconcileLocalFinaleFighters()
	ElseIf auiStageID == 400
		SetLocalPitt01Stage(450)
	ElseIf auiStageID == 450
		SetLocalPitt01Stage(460)
	ElseIf auiStageID == 460
		SetLocalPitt01Stage(500)
	ElseIf auiStageID == 500
		SetLocalPitt01Stage(ArrivalStage)
	ElseIf auiStageID == 5200
		SetLocalPitt01Stage(5400)
	ElseIf auiStageID == 5400
		SetLocalPitt01Stage(5600)
	ElseIf auiStageID == 5600
		SetLocalPitt01Stage(9000)
	ElseIf auiStageID == 9000
		ScheduleLocalPitt01Shutdown()
	ElseIf auiStageID == 10000
		CleanupLocalPitt01Mission()
		Stop()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 91
		PollLocalPitt01Phase(1, 1600, 91)
	ElseIf aiTimerID == 92
		PollLocalPitt01Phase(2, 2600, 92)
	ElseIf aiTimerID == 93
		PollLocalPitt01Phase(3, 3600, 93)
	ElseIf aiTimerID == 94
		ReconcileLocalFinaleFighters()
	ElseIf aiTimerID == 99
		If IsStageDone(9000) && !IsStageDone(10000)
			SetLocalPitt01Stage(10000)
		EndIf
	EndIf
EndEvent

Event OnQuestShutdown()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	CleanupLocalPitt01Mission()
EndEvent

Function InitializeLocalPitt01Mission()
	If IsStageDone(9000)
		If !IsStageDone(10000)
			ScheduleLocalPitt01Shutdown()
		EndIf
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If Alias_ExpeditionLeader != None && Alias_ExpeditionLeader.GetReference() != playerRef
			Alias_ExpeditionLeader.ForceRefTo(playerRef)
		EndIf
		If Alias_ExpeditionTeam != None && Alias_ExpeditionTeam.Find(playerRef) < 0
			Alias_ExpeditionTeam.AddRef(playerRef)
		EndIf
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf

	Expeditions:Master missionMaster = GetLocalPitt01Master()
	If missionMaster != None
		missionMaster.InitializeSinglePlayerAliases()
	EndIf
	RestoreLocalPitt01Selections()
	If missionMaster != None
		missionMaster.ResumeLocalMission()
	EndIf
	ResumeLocalPitt01PhaseMonitor()
	If IsStageDone(FinaleTrogAttackStage) && !IsStageDone(FinaleCompleteStage)
		RegisterLocalFinaleFighters()
		StartTimer(1.0, 94)
	EndIf
	If !IsStageDone(400) && !IsStageDone(9000)
		SetStage(400)
	EndIf
EndFunction

Function ScheduleLocalPitt01Shutdown()
	CancelTimer(99)
	StartTimer(2.0, 99)
EndFunction

Expeditions:Master Function GetLocalPitt01Master()
	Return (Self as Quest) as Expeditions:Master
EndFunction

Function RestoreLocalPitt01Selections()
	If IsStageDone(11)
		RegisterLocalPitt01Selection(11, False)
	ElseIf IsStageDone(12)
		RegisterLocalPitt01Selection(12, False)
	ElseIf IsStageDone(13)
		RegisterLocalPitt01Selection(13, False)
	EndIf
	If IsStageDone(21)
		RegisterLocalPitt01Selection(21, False)
	ElseIf IsStageDone(22)
		RegisterLocalPitt01Selection(22, False)
	ElseIf IsStageDone(23)
		RegisterLocalPitt01Selection(23, False)
	EndIf
	If IsStageDone(31)
		RegisterLocalPitt01Selection(31, False)
	ElseIf IsStageDone(32)
		RegisterLocalPitt01Selection(32, False)
	ElseIf IsStageDone(33)
		RegisterLocalPitt01Selection(33, False)
	EndIf
EndFunction

Bool Function RegisterLocalPitt01Selection(Int aiSelectionStage, Bool abResetModule)
	Expeditions:Master missionMaster = GetLocalPitt01Master()
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
		moduleFormID = 0x0064CD4E
	ElseIf aiSelectionStage == 13
		phase = 1
		moduleFormID = 0x0064EA14
	ElseIf aiSelectionStage == 21
		phase = 2
		moduleFormID = 0x0064D26C
	ElseIf aiSelectionStage == 22
		phase = 2
		moduleFormID = 0x006507B5
	ElseIf aiSelectionStage == 23
		phase = 2
		moduleFormID = 0x0064BC50
	ElseIf aiSelectionStage == 31
		phase = 3
		moduleFormID = 0x0064DD30
	ElseIf aiSelectionStage == 32
		phase = 3
		moduleFormID = 0x0064CD4D
	ElseIf aiSelectionStage == 33
		phase = 3
		moduleFormID = 0x0064C2D0
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

Function ResumeLocalPitt01PhaseMonitor()
	If IsStageDone(3000) && !IsStageDone(3600)
		StartTimer(1.0, 93)
	ElseIf IsStageDone(2000) && !IsStageDone(2600)
		StartTimer(1.0, 92)
	ElseIf IsStageDone(1000) && !IsStageDone(1600)
		StartTimer(1.0, 91)
	EndIf
EndFunction

Function PollLocalPitt01Phase(Int aiPhase, Int aiCompletionStage, Int aiTimerID)
	If IsStageDone(aiCompletionStage) || IsStageDone(9000)
		Return
	EndIf
	Expeditions:Master missionMaster = GetLocalPitt01Master()
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

Function RegisterLocalFinaleFighters()
	Int index = 0
	While FinaleFighterArray != None && index < FinaleFighterArray.Length
		FinaleFighterData fighterData = FinaleFighterArray[index]
		Actor fighterRef = fighterData.FighterAlias.GetActorReference()
		If fighterRef != None
			fighterRef.EnableNoWait()
			RegisterForRemoteEvent(fighterRef, "OnDeath")
		EndIf
		index += 1
	EndWhile
EndFunction

Bool Function IsLocalFinaleFighter(Actor akActor)
	Int index = 0
	While akActor != None && FinaleFighterArray != None && index < FinaleFighterArray.Length
		If FinaleFighterArray[index].FighterAlias.GetActorReference() == akActor
			Return True
		EndIf
		index += 1
	EndWhile
	Return False
EndFunction

Function ReconcileLocalFinaleFighters()
	Bool fighterLost = False
	Int index = 0
	While FinaleFighterArray != None && index < FinaleFighterArray.Length
		FinaleFighterData fighterData = FinaleFighterArray[index]
		Actor fighterRef = fighterData.FighterAlias.GetActorReference()
		If fighterRef == None || fighterRef.IsDead()
			fighterLost = True
		ElseIf fighterRef.IsBleedingOut()
			SetObjectiveDisplayed(fighterData.FighterReviveObjective, True)
		Else
			SetObjectiveCompleted(fighterData.FighterReviveObjective, True)
		EndIf
		index += 1
	EndWhile
	If fighterLost
		SetLocalPitt01Stage(ProtectUnionersFailedStage)
	ElseIf IsStageDone(FinaleCompleteStage)
		SetLocalPitt01Stage(4531)
	ElseIf IsStageDone(FinaleTrogAttackStage)
		StartTimer(1.0, 94)
	EndIf
EndFunction

Function SetLocalPitt01Stage(Int aiStage)
	If !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction

Function CleanupLocalPitt01Mission()
	CancelTimer(99)
	CancelTimer(91)
	CancelTimer(92)
	CancelTimer(93)
	CancelTimer(94)
	UnregisterForAllEvents()
	Expeditions:Master missionMaster = GetLocalPitt01Master()
	If missionMaster != None
		missionMaster.StopAllObjectiveModules()
	EndIf
EndFunction
