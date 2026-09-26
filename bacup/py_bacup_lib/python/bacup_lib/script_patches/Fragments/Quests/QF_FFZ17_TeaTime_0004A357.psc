Bool Function IsEventFailed()
	Return IsStageDone(600) || IsStageDone(610) || IsStageDone(620) || IsStageDone(700) || IsStageDone(800) || IsStageDone(900)
EndFunction

Bool Function IsEventResolved()
	Return IsStageDone(630) || IsEventFailed()
EndFunction

Function ResetEventObjective(Int aiObjective)
	SetObjectiveDisplayed(aiObjective, False)
	SetObjectiveCompleted(aiObjective, False)
	SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
	ResetEventObjective(200)
	ResetEventObjective(300)
	ResetEventObjective(325)
	ResetEventObjective(350)
	ResetEventObjective(500)
	ResetEventObjective(600)
	ResetEventObjective(700)
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

Function FailOpenObjectives()
	FailOpenObjective(200)
	FailOpenObjective(300)
	FailOpenObjective(325)
	FailOpenObjective(350)
	FailOpenObjective(500)
	FailOpenObjective(600)
	FailOpenObjective(700)
EndFunction

Function StartEventWave(String asWaveID)
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StartEncounterWaveByID(asWaveID)
	EndIf
EndFunction

Function StopEventWave(String asWaveID)
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StopEncounterWaveByID(asWaveID, False)
	EndIf
EndFunction

Function StopEventWaves()
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StopAllEncounterWaves(False)
	EndIf
EndFunction

Function BroadcastEventTopic(Topic akTopic)
	Quest owner = Self as Quest
	DefaultQuestEmergencyBroadcastScript broadcast = owner as DefaultQuestEmergencyBroadcastScript
	If broadcast != None
		broadcast.SendEmergencyBroadcast(akTopic, None)
	EndIf
EndFunction

Function ShowEventMessage(Message akMessage)
	If akMessage != None
		akMessage.Show()
	EndIf
EndFunction

Function StartEventTimer()
	Quest owner = Self as Quest
	B21:QuestTimer questTimer = owner as B21:QuestTimer
	If questTimer != None
		questTimer.StartQuestTimer()
	EndIf
EndFunction

Function StopEventTimer()
	Quest owner = Self as Quest
	B21:QuestTimer questTimer = owner as B21:QuestTimer
	If questTimer != None
		questTimer.StopQuestTimer()
	EndIf
EndFunction

Function SetTeapotSoundsEnabled(Bool abEnabled)
	If WhistleSound != None
		If abEnabled
			WhistleSound.Enable(False)
		Else
			WhistleSound.Disable(False)
		EndIf
	EndIf
	If BoilingSound != None
		If abEnabled
			BoilingSound.Enable(False)
		Else
			BoilingSound.Disable(False)
		EndIf
	EndIf
EndFunction

Function RepairPipe(ReferenceAlias akPipe)
	If akPipe == None
		Return
	EndIf
	ObjectReference pipeRef = akPipe.GetReference()
	If pipeRef != None
		pipeRef.ClearDestruction()
	EndIf
EndFunction

Function RepairPipes()
	RepairPipe(Alias_WaterPipe1)
	RepairPipe(Alias_WaterPipe2)
	RepairPipe(Alias_WaterPipe3)
EndFunction

Function StartWavePhase(Int aiObjective, String asWaveID, String asLegendaryWaveID)
	If IsEventResolved()
		Return
	EndIf
	; The objective goes up before spawning: an empty wave can set its follow-up stage synchronously.
	SetObjectiveDisplayed(aiObjective, True, True)
	StartEventWave(asWaveID)
	StartEventWave(asLegendaryWaveID)
EndFunction

Function AnnounceNextWave(Int aiFinishedObjective, Message akWaveMessage, Topic akWaveTopic, Int aiNextStage)
	If IsEventResolved()
		Return
	EndIf
	CompleteOpenObjective(aiFinishedObjective)
	ShowEventMessage(akWaveMessage)
	BroadcastEventTopic(akWaveTopic)
	If !IsStageDone(aiNextStage)
		SetStage(aiNextStage)
	EndIf
EndFunction

Function HandlePipeDestroyed(Int aiObjective)
	If IsEventResolved()
		Return
	EndIf
	FailOpenObjective(aiObjective)
	If IsStageDone(710) && IsStageDone(720) && IsStageDone(730) && !IsStageDone(900)
		SetStage(900)
	EndIf
EndFunction

Function FailEvent(Int aiBroadcastStage)
	If IsStageDone(630)
		Return
	EndIf
	StopEventTimer()
	StopEventWaves()
	FailOpenObjectives()
	If !IsStageDone(aiBroadcastStage)
		SetStage(aiBroadcastStage)
	EndIf
	ScheduleEventShutdown()
EndFunction

Function ScheduleEventShutdown()
	; A short delay lets the closing broadcast, objective updates and stage rewards land before Stop().
	StartTimer(10.0, 4357)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 4357 && IsRunning()
		Stop()
	EndIf
EndEvent

Function Fragment_Stage_0200_Item_00()
	CancelTimer(4357)
	ResetEventObjectives()
	SetTeapotSoundsEnabled(True)
	SetObjectiveDisplayed(200, True, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
	If IsEventResolved()
		Return
	EndIf
	CompleteOpenObjective(200)
	StartEventTimer()
	RepairPipes()
	SetObjectiveDisplayed(500, True)
	SetObjectiveDisplayed(600, True)
	SetObjectiveDisplayed(700, True)
	BroadcastEventTopic(WaveStartTopic)
	ShowEventMessage(Wave1Message)
	If !IsStageDone(325)
		SetStage(325)
	EndIf
EndFunction

Function Fragment_Stage_0325_Item_00()
	StartWavePhase(300, "Wave 1", "LegendaryWave_1")
EndFunction

Function Fragment_Stage_0400_Item_00()
	AnnounceNextWave(300, Wave2Message, SecondWaveTopic, 425)
EndFunction

Function Fragment_Stage_0425_Item_00()
	StopEventWave("Wave 1")
	StartWavePhase(325, "Wave 2", "LegendaryWave_2")
EndFunction

Function Fragment_Stage_0500_Item_00()
	AnnounceNextWave(325, Wave3Message, ThirdWaveTopic, 525)
EndFunction

Function Fragment_Stage_0525_Item_00()
	StopEventWave("Wave 2")
	StartWavePhase(350, "Wave 3", "LegendaryWave_3")
EndFunction

Function Fragment_Stage_0600_Item_00()
	If IsStageDone(630)
		Return
	EndIf
	FailOpenObjectives()
	BroadcastEventTopic(TimerElapsedTopic)
EndFunction

Function Fragment_Stage_0610_Item_00()
	If IsStageDone(630)
		Return
	EndIf
	FailOpenObjectives()
	BroadcastEventTopic(NoStartTopic)
EndFunction

Function Fragment_Stage_0620_Item_00()
	If IsStageDone(630)
		Return
	EndIf
	FailOpenObjectives()
	BroadcastEventTopic(FailureTopic)
EndFunction

Function Fragment_Stage_0630_Item_00()
	If IsEventFailed()
		Return
	EndIf
	StopEventTimer()
	CompleteOpenObjective(350)
	CompleteOpenObjective(500)
	CompleteOpenObjective(600)
	CompleteOpenObjective(700)
	BroadcastEventTopic(SuccessTopic)
	If !IsStageDone(1000)
		SetStage(1000)
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	FailEvent(600)
EndFunction

Function Fragment_Stage_0710_Item_00()
	HandlePipeDestroyed(500)
EndFunction

Function Fragment_Stage_0720_Item_00()
	HandlePipeDestroyed(600)
EndFunction

Function Fragment_Stage_0730_Item_00()
	HandlePipeDestroyed(700)
EndFunction

Function Fragment_Stage_0800_Item_00()
	FailEvent(610)
EndFunction

Function Fragment_Stage_0900_Item_00()
	FailEvent(620)
EndFunction

Function Fragment_Stage_0950_Item_00()
	StopEventWave("Player Hunters")
EndFunction

Function Fragment_Stage_1000_Item_00()
	If !IsStageDone(630)
		Return
	EndIf
	StopEventWave("Wave 3")
	If !IsStageDone(950)
		SetStage(950)
	EndIf
	ScheduleEventShutdown()
EndFunction

Function Fragment_Stage_1500_Item_00()
	CancelTimer(4357)
	StopEventTimer()
	SetTeapotSoundsEnabled(False)
	RepairPipes()
EndFunction
