; Powering Up (PowerPlantEvent 3E4E89) controller. The quest is event scoped, so it always
; arrives from PowerPlantEventNode 3EB680 with PowerPlantLocation filled from the story event's
; Location1; everything below hangs off that one alias.
;
; Repair model, all thresholds from this script's surviving constants and the plant records:
;   the three subsystem collections (ReactorIntact 2, GeneratorIntact 4, CoolingIntact 6) hold the
;   *Breakable_PowerPlant* ACTIs, which all run DefaultFixable2StateActivator and carry the
;   "Repair" activate text, so the player repairs one by activating it;
;   the event damages each subsystem down to CONST_SubsystemFailureAllowedPercent (30) on start,
;   basic repairs complete at CONST_ShowSubsystemQTThresholdPercent (50) - the wiki's "basic
;   repairs (50%)" - which also flips ShowSubsystemXQTs so the objective targets on the
;   *Destroyed collections start marking the leftovers, and full repairs complete at 100%.
;
; FO76 services with no FO4 equivalent: the SQ_Master/PowerPlantEBSTopics event broadcast VO and
; the FO76 "<Token=...>" terminal readout are not reproduced; PowerPlantRewardLevel is still set
; (0 basic, 1 full) for reward-tier conditions.

powerplantsubsystemrefcolscript Function GetSubsystem(Int aiSubsystemID)
	If SubsystemRefCollections == None
		Return None
	EndIf
	Int index = 0
	While index < SubsystemRefCollections.Length
		powerplantsubsystemrefcolscript subsystem = SubsystemRefCollections[index]
		If subsystem != None && subsystem.SubsystemID == aiSubsystemID
			Return subsystem
		EndIf
		index += 1
	EndWhile
	Return None
EndFunction

Int Function ReadSubsystemIntactPercent(Int aiSubsystemID)
	powerplantsubsystemrefcolscript subsystem = GetSubsystem(aiSubsystemID)
	If subsystem == None
		Return 100
	EndIf
	Return subsystem.GetIntactPercent()
EndFunction

; ControlRoomTerminal is alias 22 (ALST 22); the quest script carries no binding for it.
ObjectReference Function GetControlRoomTerminalRef()
	ReferenceAlias terminalAlias = GetAlias(22) as ReferenceAlias
	If terminalAlias == None
		Return None
	EndIf
	Return terminalAlias.GetReference()
EndFunction

PowerPlantTerminalScript Function GetControlRoomTerminal()
	ObjectReference terminalRef = GetControlRoomTerminalRef()
	If terminalRef == None
		Return None
	EndIf
	Return terminalRef.GetBaseObject() as PowerPlantTerminalScript
EndFunction

Function ApplyTerminalRepairNeeded(Bool abRestartOffered)
	ObjectReference terminalRef = GetControlRoomTerminalRef()
	PowerPlantTerminalScript plantTerminal = GetControlRoomTerminal()
	If plantTerminal != None
		plantTerminal.SetPowerPlantTerminalNeedsRepair(terminalRef, abRestartOffered)
	EndIf
EndFunction

Function ApplyTerminalReadyForRestart()
	ObjectReference terminalRef = GetControlRoomTerminalRef()
	PowerPlantTerminalScript plantTerminal = GetControlRoomTerminal()
	If plantTerminal != None
		plantTerminal.SetPowerPlantTerminalReadyForRestart(terminalRef)
	EndIf
EndFunction

Function ApplyTerminalOnline()
	ObjectReference terminalRef = GetControlRoomTerminalRef()
	PowerPlantTerminalScript plantTerminal = GetControlRoomTerminal()
	If plantTerminal != None
		plantTerminal.SetPowerPlantTerminalOnline(terminalRef)
	EndIf
EndFunction

Function SetSupportCollectionEnabled(RefCollectionAlias akCollection, Bool abEnabled)
	If akCollection == None
		Return
	EndIf
	If abEnabled
		akCollection.EnableAll(False)
	Else
		akCollection.DisableAll(False)
	EndIf
EndFunction

; The support objects are the running-plant dressing (reactor machinery, cooling tower steam FX,
; general props). They go dark while the plant is offline and come back when it restarts.
Function SetSupportObjectsEnabled(Bool abEnabled)
	SetSupportCollectionEnabled(ReactorSupport, abEnabled)
	SetSupportCollectionEnabled(GeneratorSupport, abEnabled)
	SetSupportCollectionEnabled(GeneralSupport, abEnabled)
	If CoolingTowerSupport != None
		Int index = 0
		While index < CoolingTowerSupport.Length
			SetSupportCollectionEnabled(CoolingTowerSupport[index], abEnabled)
			index += 1
		EndWhile
	EndIf
EndFunction

Function ApplyPowerPlantNames()
	If PPMasterQI == None || PowerPlantIndex < 0
		Return
	EndIf
	Location shortName = PPMasterQI.GetPowerPlantShortNameLocation(PowerPlantIndex)
	If PowerPlantShortNameLocation != None && shortName != None
		PowerPlantShortNameLocation.ForceLocationTo(shortName)
	EndIf
	Location titleName = PPMasterQI.GetPowerPlantTitleNameLocation(PowerPlantIndex)
	If PowerPlantTitleNameLocation != None && titleName != None
		PowerPlantTitleNameLocation.ForceLocationTo(titleName)
	EndIf
EndFunction

Int Function GetSubsystemDamageTargetPercent(Int aiSubsystemID)
	Int storedPercent = 0
	If PPMasterQI != None && PowerPlantIndex >= 0
		storedPercent = PPMasterQI.GetSubsystemIntactPercent(PowerPlantIndex, aiSubsystemID)
	EndIf
	If storedPercent < CONST_SubsystemFailureAllowedPercent
		storedPercent = CONST_SubsystemFailureAllowedPercent
	EndIf
	Return storedPercent
EndFunction

Function BreakSubsystems()
	If SubsystemRefCollections == None
		Return
	EndIf
	Int index = 0
	While index < SubsystemRefCollections.Length
		powerplantsubsystemrefcolscript subsystem = SubsystemRefCollections[index]
		If subsystem != None
			subsystem.DestroySubsystemToThreshold(GetSubsystemDamageTargetPercent(subsystem.SubsystemID) as Float, True)
		EndIf
		index += 1
	EndWhile
EndFunction

Function RepairSubsystems()
	If SubsystemRefCollections == None
		Return
	EndIf
	Int index = 0
	While index < SubsystemRefCollections.Length
		powerplantsubsystemrefcolscript subsystem = SubsystemRefCollections[index]
		If subsystem != None
			subsystem.RepairSubsystem()
		EndIf
		index += 1
	EndWhile
EndFunction

Function UpdateSubsystemObjectives(Int aiBasicObjective, Int aiFullObjective, Int aiIntactPercent)
	If aiIntactPercent < CONST_ShowSubsystemQTThresholdPercent
		SetObjectiveDisplayed(aiBasicObjective, True)
		Return
	EndIf
	SetObjectiveCompleted(aiBasicObjective, True)
	; Forced once, so the "(Optional) Fully repair" targets re-evaluate their
	; ShowSubsystemXQTs condition now that basic repairs are in.
	If !IsObjectiveDisplayed(aiFullObjective)
		SetObjectiveDisplayed(aiFullObjective, True, True)
	EndIf
	If aiIntactPercent >= 100
		SetObjectiveCompleted(aiFullObjective, True)
	EndIf
EndFunction

Function RefreshRepairObjectives()
	If isShuttingDown || IsStageDone(CONST_RestartStage)
		Return
	EndIf
	SetObjectiveDisplayed(20, True)
	UpdateSubsystemObjectives(CONST_Objective_RepairReactorBasic, CONST_Objective_RepairReactorFull, SubsystemReactorIntactPercent)
	UpdateSubsystemObjectives(CONST_Objective_RepairGeneratorBasic, CONST_Objective_RepairGeneratorFull, SubsystemGeneratorIntactPercent)
	UpdateSubsystemObjectives(CONST_Objective_RepairCoolingBasic, CONST_Objective_RepairCoolingFull, SubsystemCoolingIntactPercent)
	If !AnySubsystemHasFailed
		SetObjectiveCompleted(20, True)
		If !IsObjectiveDisplayed(30)
			SetObjectiveDisplayed(30, True, True)
		EndIf
		If SubsystemAverageIntactPercent >= 100
			SetObjectiveCompleted(30, True)
		EndIf
	EndIf
EndFunction

Function RefreshSubsystemState()
	If !allowEventStateUpdates || isShuttingDown || lock_EventState
		Return
	EndIf
	lock_EventState = True

	Int reactorPercent = ReadSubsystemIntactPercent(CONST_ReactorSubsystemID)
	Int generatorPercent = ReadSubsystemIntactPercent(CONST_GeneratorSubsystemID)
	Int coolingPercent = ReadSubsystemIntactPercent(CONST_CoolingSubsystemID)
	Bool percentsChanged = reactorPercent != SubsystemReactorIntactPercent || generatorPercent != SubsystemGeneratorIntactPercent || coolingPercent != SubsystemCoolingIntactPercent

	SubsystemReactorIntactPercent = reactorPercent
	SubsystemGeneratorIntactPercent = generatorPercent
	SubsystemCoolingIntactPercent = coolingPercent
	SubsystemAverageIntactPercent = (reactorPercent + generatorPercent + coolingPercent) / CONST_SubsystemCount

	SubsystemReactorHasFailed = reactorPercent < CONST_ShowSubsystemQTThresholdPercent
	SubsystemGeneratorHasFailed = generatorPercent < CONST_ShowSubsystemQTThresholdPercent
	SubsystemCoolingHasFailed = coolingPercent < CONST_ShowSubsystemQTThresholdPercent
	AnySubsystemHasFailed = SubsystemReactorHasFailed || SubsystemGeneratorHasFailed || SubsystemCoolingHasFailed

	; The leftover breakables are only marked individually once basic repairs are in.
	ShowSubsystemReactorQTs = !SubsystemReactorHasFailed && reactorPercent < 100
	ShowSubsystemGeneratorQTs = !SubsystemGeneratorHasFailed && generatorPercent < 100
	ShowSubsystemCoolingQTs = !SubsystemCoolingHasFailed && coolingPercent < 100

	If PPMasterQI != None && PowerPlantIndex >= 0
		PPMasterQI.SetSubsystemIntactPercent(PowerPlantIndex, CONST_ReactorSubsystemID, reactorPercent, SubsystemReactorHasFailed)
		PPMasterQI.SetSubsystemIntactPercent(PowerPlantIndex, CONST_GeneratorSubsystemID, generatorPercent, SubsystemGeneratorHasFailed)
		PPMasterQI.SetSubsystemIntactPercent(PowerPlantIndex, CONST_CoolingSubsystemID, coolingPercent, SubsystemCoolingHasFailed)
	EndIf

	If SubsystemAverageIntactPercent >= 100
		PowerPlantRewardLevel = 1
	Else
		PowerPlantRewardLevel = 0
	EndIf

	lock_EventState = False

	RefreshRepairObjectives()
	If percentsChanged
		SetStage(CONST_UpdateRepairObjectiveStage)
	EndIf
	AdvanceRepairStage()
EndFunction

Function AdvanceRepairStage()
	If isShuttingDown || IsStageDone(CONST_RestartStage)
		Return
	EndIf
	If !AnySubsystemHasFailed && SubsystemAverageIntactPercent >= 100
		If !IsStageDone(CONST_RepairCompleteStage)
			SetStage(CONST_RepairCompleteStage)
		EndIf
	ElseIf !AnySubsystemHasFailed
		If !IsStageDone(CONST_RepairAboveThresholdStage)
			SetStage(CONST_RepairAboveThresholdStage)
		EndIf
	ElseIf !IsStageDone(CONST_RepairBelowThresholdStage)
		SetStage(CONST_RepairBelowThresholdStage)
	EndIf

	If !AnySubsystemHasFailed
		If !IsStageDone(CONST_RestartPermittedStage)
			SetStage(CONST_RestartPermittedStage)
		EndIf
	ElseIf !IsStageDone(CONST_RestartNotPermittedStage)
		SetStage(CONST_RestartNotPermittedStage)
	EndIf
EndFunction

; Called by PowerPlantSubsystemRefColScript after a repair or after the start-of-event damage.
Function NotifySubsystemStateChanged(Int aiSubsystemID)
	RefreshSubsystemState()
EndFunction

; Called by PowerPlantTerminalScript when the player runs the master control terminal's restart.
Function RestartPowerPlant()
	If isShuttingDown || !IsRunning() || AnySubsystemHasFailed
		Return
	EndIf
	If !IsStageDone(CONST_RestartPermittedStage) || IsStageDone(CONST_RestartStage)
		Return
	EndIf
	SetStage(CONST_RestartStage)
EndFunction

Function FinishPowerPlantRestart()
	If PPMasterQI != None && PowerPlantIndex >= 0
		PPMasterQI.NotifyPowerPlantRestarted(PowerPlantIndex)
	EndIf
	RepairSubsystems()
	ApplyTerminalOnline()
	CancelTimer(CONST_ToggleSupportObjectsOffTimerID)
	StartTimer(1.0, CONST_ToggleSupportObjectsOnTimerID)
	StartTimer(10.0, 13)
EndFunction

Function StartEventTimers()
	Quest owner = Self as Quest
	B21:QuestTimer questTimer = owner as B21:QuestTimer
	If questTimer != None
		RegisterForCustomEvent(questTimer, "QuestTimerEnded")
		questTimer.StartQuestTimer()
	EndIf
	CancelTimer(CONST_LateJoinCutoffTimerID)
	StartTimer((CONST_LateJoinCutoffTime_Minutes * 60) as Float, CONST_LateJoinCutoffTimerID)
EndFunction

Function CancelEventTimers()
	CancelTimer(CONST_LateJoinCutoffTimerID)
	CancelTimer(CONST_ToggleSupportObjectsOnTimerID)
	CancelTimer(CONST_ToggleSupportObjectsOffTimerID)
	CancelTimer(13)
	Quest owner = Self as Quest
	B21:QuestTimer questTimer = owner as B21:QuestTimer
	If questTimer != None
		questTimer.StopQuestTimer()
	EndIf
EndFunction

Function InitializePowerPlantEvent()
	If hasInitialized
		Return
	EndIf
	hasInitialized = True
	isShuttingDown = False
	lock_EventState = False
	PPMasterQI = PowerPlantMaster as powerplantmasterquestscript
	If PowerPlantLocation != None
		PowerPlantLoc = PowerPlantLocation.GetLocation()
	EndIf
	PowerPlantIndex = -1
	If PPMasterQI != None
		PowerPlantIndex = PPMasterQI.GetPowerPlantIndexForLocation(PowerPlantLoc)
		If PowerPlantIndex >= 0
			PPMasterQI.RegisterPowerPlantEvent(PowerPlantIndex, Self)
		EndIf
	EndIf
	ApplyPowerPlantNames()

	SetObjectiveDisplayed(20, True, True)
	SetObjectiveDisplayed(CONST_Objective_RepairReactorBasic, True, True)
	SetObjectiveDisplayed(CONST_Objective_RepairGeneratorBasic, True, True)
	SetObjectiveDisplayed(CONST_Objective_RepairCoolingBasic, True, True)

	SetSupportObjectsEnabled(False)
	ApplyTerminalRepairNeeded(True)
	BreakSubsystems()
	allowEventStateUpdates = True
	RefreshSubsystemState()
	StartEventTimers()
EndFunction

Event OnQuestInit()
	InitializePowerPlantEvent()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == 10
		InitializePowerPlantEvent()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == CONST_LateJoinCutoffTimerID
		If IsRunning() && !isShuttingDown && !IsStageDone(CONST_LateJoinCutoffStage)
			SetStage(CONST_LateJoinCutoffStage)
		EndIf
	ElseIf aiTimerID == CONST_ToggleSupportObjectsOnTimerID
		SetSupportObjectsEnabled(True)
	ElseIf aiTimerID == CONST_ToggleSupportObjectsOffTimerID
		SetSupportObjectsEnabled(False)
	ElseIf aiTimerID == 13
		If IsRunning()
			Stop()
		EndIf
	EndIf
EndEvent

Event B21:QuestTimer.QuestTimerEnded(B21:QuestTimer akSender, Var[] akArgs)
	; Stage 250 carries the FailQuest flag; CONST_FailureStage (200) is not a stage on this quest.
	If IsRunning() && !isShuttingDown && !IsStageDone(CONST_RestartStage) && !IsStageDone(250)
		SetStage(250)
	EndIf
EndEvent

Event OnQuestShutdown()
	isShuttingDown = True
	allowEventStateUpdates = False
	CancelEventTimers()
	Bool restarted = IsStageDone(CONST_RestartStage)
	If !restarted
		ApplyTerminalRepairNeeded(False)
	EndIf
	If PPMasterQI != None && PowerPlantIndex >= 0
		If !restarted
			PPMasterQI.NotifyPowerPlantEventFailed(PowerPlantIndex)
		EndIf
		PPMasterQI.NotifyPowerPlantEventStopped(PowerPlantIndex)
	EndIf
	hasInitialized = False
EndEvent
