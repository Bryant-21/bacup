Event OnQuestInit()
	InitializeSinglePlayerAliases()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf

	ObjectiveRandomizer_Script = (Self as Quest) as Expeditions:ObjectiveRandomizer
	If ObjectiveRandomizer_Script != None
		ObjectiveRandomizer_Script.ApplyDeterministicSelections()
	EndIf
	ResumeLocalMission()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer()
		InitializeSinglePlayerAliases()
		ResumeLocalMission()
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == ST_OBJ_A_PHASE_START
		StartObjectiveModuleForPhase(1)
	ElseIf auiStageID == ST_OBJ_A_PHASE_COMPLETE
		StopObjectiveModuleForPhase(1)
	ElseIf auiStageID == ST_OBJ_B_PHASE_START
		StartObjectiveModuleForPhase(2)
	ElseIf auiStageID == ST_OBJ_B_PHASE_COMPLETE
		StopObjectiveModuleForPhase(2)
	ElseIf auiStageID == ST_OBJ_C_PHASE_START
		StartObjectiveModuleForPhase(3)
	ElseIf auiStageID == ST_OBJ_C_PHASE_COMPLETE
		StopObjectiveModuleForPhase(3)
	ElseIf auiStageID == ST_QUEST_COMPLETE
		StopAllObjectiveModules()
	EndIf
EndEvent

Event OnQuestShutdown()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	StopAllObjectiveModules()
	ClearLocalRuntimeState()
EndEvent

Function InitializeSinglePlayerAliases()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf

	If Alias_ExpeditionLeader != None && Alias_ExpeditionLeader.GetReference() != playerRef
		Alias_ExpeditionLeader.ForceRefTo(playerRef)
	EndIf
	If Alias_ExpeditionTeam != None && Alias_ExpeditionTeam.Find(playerRef) < 0
		Alias_ExpeditionTeam.AddRef(playerRef)
	EndIf
EndFunction

Bool Function RegisterObjectiveModule(Int aiPhase, Quest akModuleQuest, Bool abResetModule = False)
	If aiPhase < 1 || aiPhase > 3 || akModuleQuest == None
		Return False
	EndIf

	Quest previousModule = GetObjectiveModuleQuest(aiPhase)
	If previousModule != None && previousModule != akModuleQuest && previousModule.IsRunning()
		previousModule.Stop()
	EndIf

	If abResetModule
		If !akModuleQuest.IsStopped()
			Return False
		EndIf
		akModuleQuest.Reset()
	EndIf

	If aiPhase == 1
		Obj_A_ModuleQI = akModuleQuest
	ElseIf aiPhase == 2
		Obj_B_ModuleQI = akModuleQuest
	Else
		Obj_C_ModuleQI = akModuleQuest
	EndIf
	Return True
EndFunction

Quest Function GetObjectiveModuleQuest(Int aiPhase)
	If aiPhase == 1
		Return Obj_A_ModuleQI
	ElseIf aiPhase == 2
		Return Obj_B_ModuleQI
	ElseIf aiPhase == 3
		Return Obj_C_ModuleQI
	EndIf
	Return None
EndFunction

Function ResumeLocalMission()
	If bResumingMission
		Return
	EndIf

	bResumingMission = True
	If IsStageDone(ST_QUEST_COMPLETE)
		StopAllObjectiveModules()
	ElseIf IsStageDone(ST_OBJ_C_PHASE_START) && !IsStageDone(ST_OBJ_C_PHASE_COMPLETE)
		StartObjectiveModuleForPhase(3)
	ElseIf IsStageDone(ST_OBJ_B_PHASE_START) && !IsStageDone(ST_OBJ_B_PHASE_COMPLETE)
		StartObjectiveModuleForPhase(2)
	ElseIf IsStageDone(ST_OBJ_A_PHASE_START) && !IsStageDone(ST_OBJ_A_PHASE_COMPLETE)
		StartObjectiveModuleForPhase(1)
	EndIf
	bResumingMission = False
EndFunction

Function StartObjectiveModuleForPhase(Int aiPhase)
	Quest moduleQuest = GetObjectiveModuleQuest(aiPhase)
	If moduleQuest != None && !moduleQuest.IsRunning() && !moduleQuest.IsStarting()
		ForceLocalModuleLocation(moduleQuest)
		moduleQuest.Start()
	EndIf
EndFunction

Function ForceLocalModuleLocation(Quest moduleQuest)
	Actor playerRef = Game.GetPlayer()
	LocationAlias moduleLocation = moduleQuest.GetAlias(3) as LocationAlias
	If playerRef != None && moduleLocation != None
		Location currentLocation = playerRef.GetCurrentLocation()
		If currentLocation != None
			moduleLocation.ForceLocationTo(currentLocation)
		EndIf
	EndIf
EndFunction

Function StopObjectiveModuleForPhase(Int aiPhase)
	Quest moduleQuest = GetObjectiveModuleQuest(aiPhase)
	If moduleQuest != None && (moduleQuest.IsRunning() || moduleQuest.IsStarting())
		moduleQuest.Stop()
	EndIf
EndFunction

Function StopAllObjectiveModules()
	StopObjectiveModuleForPhase(1)
	StopObjectiveModuleForPhase(2)
	StopObjectiveModuleForPhase(3)
	If PilotDialogue_ModuleQI != None && (PilotDialogue_ModuleQI.IsRunning() || PilotDialogue_ModuleQI.IsStarting())
		PilotDialogue_ModuleQI.Stop()
	EndIf
EndFunction

Function ClearLocalRuntimeState()
	Obj_A_ModuleQI = None
	Obj_B_ModuleQI = None
	Obj_C_ModuleQI = None
	PilotDialogue_ModuleQI = None
	ObjectiveRandomizer_Script = None
	bResumingMission = False
	If Alias_ExpeditionTeam != None
		Alias_ExpeditionTeam.RemoveAll()
	EndIf
	If Alias_ExpeditionLeader != None
		Alias_ExpeditionLeader.Clear()
	EndIf
EndFunction
