DefaultQuestEncounterWaveScript Function EncounterWaves()
	Quest owner = Self as Quest
	Return owner as DefaultQuestEncounterWaveScript
EndFunction

Bool Function EventEnded()
	Return IsStageDone(1000) || IsStageDone(1100) || IsStageDone(9991)
EndFunction

Function StartWave(String asIDString)
	If EventEnded()
		Return
	EndIf
	DefaultQuestEncounterWaveScript waveScript = EncounterWaves()
	If waveScript != None
		waveScript.StartEncounterWaveByID(asIDString)
	EndIf
EndFunction

Function StartBossWave(String asIDString)
	If EventEnded() || IsStageDone(800)
		Return
	EndIf
	StartWave(asIDString)
	SetStage(800)
EndFunction

Bool Function IsHeartClosed()
	SFS08_Heart_StranglerHeartAliasScript heartAlias = Alias_StranglerHeart as SFS08_Heart_StranglerHeartAliasScript
	Return heartAlias != None && heartAlias.IsHeartClosed()
EndFunction

Function EnsureHeartFound()
	If !IsStageDone(100)
		SetStage(100)
	EndIf
EndFunction

Function CompleteObjectiveIfOpen(Int aiObjective)
	If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveCompleted(aiObjective, True)
	EndIf
EndFunction

Function FailObjectiveIfOpen(Int aiObjective)
	If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveFailed(aiObjective, True)
	EndIf
EndFunction

Function ShowHeartClosedObjectives()
	If EventEnded()
		Return
	EndIf
	SetObjectiveDisplayed(100, False)
	SetObjectiveDisplayed(200, True, True)
	SetObjectiveDisplayed(250, True, True)
EndFunction

Function ShowHeartOpenObjectives()
	If EventEnded()
		Return
	EndIf
	SetObjectiveDisplayed(200, False)
	SetObjectiveDisplayed(250, False)
	If !IsStageDone(700) && !IsHeartClosed()
		SetObjectiveDisplayed(100, True, True)
	EndIf
EndFunction

Function TryCompleteEvent()
	If IsStageDone(700) && IsStageDone(900) && !EventEnded()
		SetStage(1000)
	EndIf
EndFunction

Function FinishEvent(Bool abFailed)
	If abFailed
		FailObjectiveIfOpen(70)
		FailObjectiveIfOpen(100)
		FailObjectiveIfOpen(200)
		FailObjectiveIfOpen(250)
		FailObjectiveIfOpen(800)
		FailObjectiveIfOpen(850)
	Else
		SetObjectiveDisplayed(200, False)
		SetObjectiveDisplayed(250, False)
		CompleteObjectiveIfOpen(70)
		CompleteObjectiveIfOpen(100)
		CompleteObjectiveIfOpen(800)
		CompleteObjectiveIfOpen(850)
	EndIf
	SFS08_Heart_QuestScript eventScript = (Self as Quest) as SFS08_Heart_QuestScript
	If eventScript != None
		eventScript.ScheduleEventShutdown()
	Else
		Stop()
	EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
	SetObjectiveDisplayed(70, True, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
	CompleteObjectiveIfOpen(70)
	If !IsStageDone(700) && !IsHeartClosed()
		SetObjectiveDisplayed(100, True, True)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	EnsureHeartFound()
	If !IsStageDone(225)
		SetStage(225)
	EndIf
EndFunction

Function Fragment_Stage_0225_Item_00()
	ShowHeartClosedObjectives()
	StartWave("95% Health Mirelurks Wave")
EndFunction

Function Fragment_Stage_0250_Item_00()
	ShowHeartOpenObjectives()
EndFunction

Function Fragment_Stage_0300_Item_00()
	EnsureHeartFound()
	If !IsStageDone(325)
		SetStage(325)
	EndIf
EndFunction

Function Fragment_Stage_0325_Item_00()
	ShowHeartClosedObjectives()
	StartWave("75% Health Ghouls Wave A")
	StartWave("75% Health Ghouls Wave B")
EndFunction

Function Fragment_Stage_0330_Item_00()
	If IsStageDone(340) && !IsStageDone(350)
		SetStage(350)
	EndIf
EndFunction

Function Fragment_Stage_0340_Item_00()
	If IsStageDone(330) && !IsStageDone(350)
		SetStage(350)
	EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
	ShowHeartOpenObjectives()
EndFunction

Function Fragment_Stage_0400_Item_00()
	EnsureHeartFound()
	If !IsStageDone(425)
		SetStage(425)
	EndIf
EndFunction

Function Fragment_Stage_0425_Item_00()
	ShowHeartClosedObjectives()
	StartWave("50% Health Mirelurks Wave")
EndFunction

Function Fragment_Stage_0450_Item_00()
	ShowHeartOpenObjectives()
EndFunction

Function Fragment_Stage_0500_Item_00()
	If !IsStageDone(25)
		SetStage(25)
	EndIf
	StartBossWave("25% Health Queen Wave")
EndFunction

Function Fragment_Stage_0500_Item_01()
	If !IsStageDone(25)
		SetStage(25)
	EndIf
	StartBossWave("25% Health Grafton Wave")
EndFunction

Function Fragment_Stage_0600_Item_00()
	StartWave("10% Health Mix Wave")
EndFunction

Function Fragment_Stage_0650_Item_00()
	StartBossWave("25% Health Queen Wave")
EndFunction

Function Fragment_Stage_0650_Item_01()
	StartBossWave("25% Health Grafton Wave")
EndFunction

Function Fragment_Stage_0700_Item_00()
	If EventEnded()
		Return
	EndIf
	SetObjectiveDisplayed(200, False)
	SetObjectiveDisplayed(250, False)
	If !IsObjectiveCompleted(100)
		SetObjectiveCompleted(100, True)
	EndIf
	TryCompleteEvent()
EndFunction

Function Fragment_Stage_0800_Item_00()
	If EventEnded()
		Return
	EndIf
	SetObjectiveDisplayed(800, True, True)
EndFunction

Function Fragment_Stage_0800_Item_01()
	If EventEnded()
		Return
	EndIf
	SetObjectiveDisplayed(850, True, True)
EndFunction

Function Fragment_Stage_0900_Item_00()
	If EventEnded()
		Return
	EndIf
	CompleteObjectiveIfOpen(800)
	CompleteObjectiveIfOpen(850)
	TryCompleteEvent()
EndFunction

Function Fragment_Stage_1000_Item_00()
	FinishEvent(False)
EndFunction

Function Fragment_Stage_1100_Item_00()
	FinishEvent(True)
EndFunction

Function Fragment_Stage_9991_Item_00()
	FailObjectiveIfOpen(70)
	Stop()
EndFunction
