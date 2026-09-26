E09A_QuestScript Function EventScript()
	Quest owner = Self as Quest
	Return owner as E09A_QuestScript
EndFunction

DefaultQuestEncounterWaveScript Function WaveScript()
	Quest owner = Self as Quest
	Return owner as DefaultQuestEncounterWaveScript
EndFunction

Function ResetObjective(Int aiObjective)
	SetObjectiveDisplayed(aiObjective, False)
	SetObjectiveCompleted(aiObjective, False)
	SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
	ResetObjective(10)
	ResetObjective(20)
	ResetObjective(30)
	ResetObjective(40)
	ResetObjective(50)
	ResetObjective(60)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
	If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveCompleted(aiObjective, True)
	EndIf
EndFunction

Function FailOpenObjective(Int aiObjective)
	If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveFailed(aiObjective, True)
	EndIf
EndFunction

Function FailOpenBossObjectives()
	FailOpenObjective(20)
	FailOpenObjective(30)
	FailOpenObjective(40)
	FailOpenObjective(50)
	FailOpenObjective(60)
EndFunction

Function ShowWaitForTitanObjective()
	; Objective 30 is shown again between every phase, so it reopens before each display.
	ResetObjective(30)
	SetObjectiveDisplayed(30, True, True)
EndFunction

Function StartAdditionWaves(String asMoleMinerWaveID)
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves == None
		Return
	EndIf
	waves.StartEncounterWaveByID("Wave1_Ultramites")
	If asMoleMinerWaveID != ""
		waves.StartEncounterWaveByID(asMoleMinerWaveID)
	EndIf
EndFunction

Function StartEndOfRoundWave()
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None
		waves.StartEncounterWaveByID("End of round Explosion")
	EndIf
EndFunction

Function BeginBossPhase(Int aiPhase)
	E09A_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.BeginBossPhase(aiPhase)
	EndIf
EndFunction

Function BeginPhaseTransition(Int aiCompletedObjective)
	CompleteOpenObjective(aiCompletedObjective)
	ShowWaitForTitanObjective()
	StartEndOfRoundWave()
	E09A_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.BeginPhaseTransition()
	EndIf
EndFunction

Function FailEvent()
	If !IsStageDone(900) && !IsStageDone(999)
		SetStage(999)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	ResetEventObjectives()
	E09A_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.BeginEvent()
	EndIf
	SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	CompleteOpenObjective(10)
	BeginBossPhase(0)
	SetObjectiveDisplayed(20, True, True)
	If !IsStageDone(201)
		SetStage(201)
	EndIf
EndFunction

Function Fragment_Stage_0201_Item_00()
	StartAdditionWaves("")
EndFunction

Function Fragment_Stage_0205_Item_00()
	BeginPhaseTransition(20)
EndFunction

Function Fragment_Stage_0225_Item_00()
	CompleteOpenObjective(20)
	CompleteOpenObjective(30)
	BeginBossPhase(1)
	SetObjectiveDisplayed(40, True, True)
	If !IsStageDone(226)
		SetStage(226)
	EndIf
EndFunction

Function Fragment_Stage_0226_Item_00()
	StartAdditionWaves("Wave2_Moleminers")
EndFunction

Function Fragment_Stage_0230_Item_00()
	BeginPhaseTransition(40)
EndFunction

Function Fragment_Stage_0250_Item_00()
	CompleteOpenObjective(40)
	CompleteOpenObjective(30)
	BeginBossPhase(2)
	SetObjectiveDisplayed(50, True, True)
	If !IsStageDone(251)
		SetStage(251)
	EndIf
EndFunction

Function Fragment_Stage_0251_Item_00()
	StartAdditionWaves("wave3_MM Lazers")
EndFunction

Function Fragment_Stage_0255_Item_00()
	BeginPhaseTransition(50)
EndFunction

Function Fragment_Stage_0275_Item_00()
	CompleteOpenObjective(50)
	CompleteOpenObjective(30)
	BeginBossPhase(3)
	SetObjectiveDisplayed(60, True, True)
	If !IsStageDone(276)
		SetStage(276)
	EndIf
EndFunction

Function Fragment_Stage_0276_Item_00()
	StartAdditionWaves("wave4_MM Jugg")
EndFunction

Function Fragment_Stage_0400_Item_00()
	CompleteOpenObjective(20)
	CompleteOpenObjective(30)
	CompleteOpenObjective(40)
	CompleteOpenObjective(50)
	CompleteOpenObjective(60)
	If !IsStageDone(900) && !IsStageDone(999)
		SetStage(900)
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	FailOpenObjective(10)
	FailEvent()
EndFunction

Function Fragment_Stage_0550_Item_00()
	FailOpenBossObjectives()
	E09A_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.EndBossFight()
	EndIf
	FailEvent()
EndFunction

Function Fragment_Stage_0900_Item_00()
	E09A_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.FinishEvent(True)
	Else
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_0999_Item_00()
	FailOpenObjective(10)
	FailOpenBossObjectives()
	E09A_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.EndBossFight()
		eventScript.FinishEvent(False)
	Else
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	E09A_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.CleanupEventWorld()
	EndIf
	Stop()
EndFunction
