Bool Function IsEventOver()
	Return IsStageDone(999) || IsStageDone(1000) || IsStageDone(1001) || IsStageDone(9991)
EndFunction

Bool Function IsWaveProgressClosed()
	Return IsStageDone(900) || IsEventOver()
EndFunction

Function ResetEventObjective(Int aiObjective)
	SetObjectiveDisplayed(aiObjective, False)
	SetObjectiveCompleted(aiObjective, False)
	SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
	ResetEventObjective(100)
	ResetEventObjective(101)
	ResetEventObjective(160)
	ResetEventObjective(165)
	ResetEventObjective(170)
	ResetEventObjective(180)
	ResetEventObjective(300)
	ResetEventObjective(360)
	ResetEventObjective(365)
	ResetEventObjective(370)
	ResetEventObjective(380)
	ResetEventObjective(500)
	ResetEventObjective(560)
	ResetEventObjective(565)
	ResetEventObjective(570)
	ResetEventObjective(900)
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

Function FailOpenWaveObjectives()
	FailOpenObjective(101)
	FailOpenObjective(160)
	FailOpenObjective(165)
	FailOpenObjective(170)
	FailOpenObjective(180)
	FailOpenObjective(300)
	FailOpenObjective(360)
	FailOpenObjective(365)
	FailOpenObjective(370)
	FailOpenObjective(380)
	FailOpenObjective(500)
	FailOpenObjective(560)
	FailOpenObjective(565)
	FailOpenObjective(570)
EndFunction

Function StartEventWave(String asWaveID)
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StartEncounterWaveByID(asWaveID)
	EndIf
EndFunction

Function StartWavePhase(Int aiTimedObjective, Int aiHordeObjective, String asFirstSubwaveID)
	If IsWaveProgressClosed()
		Return
	EndIf
	; Objectives go up before spawning: an empty fallback wave can set its follow-up stage synchronously.
	SetObjectiveDisplayed(aiTimedObjective, True, True)
	SetObjectiveDisplayed(aiHordeObjective, True, True)
	StartEventWave(asFirstSubwaveID)
EndFunction

Function StartLeaderSubwave(Int aiLeaderObjective, String asSubwaveID, String asLeaderWaveID)
	If IsWaveProgressClosed()
		Return
	EndIf
	SetObjectiveDisplayed(aiLeaderObjective, True, True)
	StartEventWave(asSubwaveID)
	StartEventWave(asLeaderWaveID)
EndFunction

Function FinishWaveIfDefeated(Int aiLeaderStage, Int aiHordeStage, Int aiTimedObjective, Int aiLastChanceObjective, Int aiRewardStage, Int aiNextStage)
	If !IsStageDone(aiLeaderStage) || !IsStageDone(aiHordeStage) || IsWaveProgressClosed()
		Return
	EndIf
	CompleteOpenObjective(aiTimedObjective)
	CompleteOpenObjective(aiLastChanceObjective)
	If aiRewardStage >= 0 && !IsStageDone(aiRewardStage)
		SetStage(aiRewardStage)
	EndIf
	If !IsStageDone(aiNextStage)
		SetStage(aiNextStage)
	EndIf
EndFunction

Function BeginLastChance(Int aiTimedObjective, Int aiLastChanceObjective, Int aiWaveDefeatedStage)
	If IsStageDone(aiWaveDefeatedStage) || IsWaveProgressClosed()
		Return
	EndIf
	FailOpenObjective(aiTimedObjective)
	SetObjectiveDisplayed(aiLastChanceObjective, True, True)
EndFunction

Function BeginDowntime(Int aiDowntimeObjective)
	If IsWaveProgressClosed()
		Return
	EndIf
	SetObjectiveDisplayed(aiDowntimeObjective, True, True)
EndFunction

Function EndDowntime(Int aiDowntimeObjective, Int aiNextWaveStage)
	CompleteOpenObjective(aiDowntimeObjective)
	If !IsWaveProgressClosed() && !IsStageDone(aiNextWaveStage)
		SetStage(aiNextWaveStage)
	EndIf
EndFunction

Function BindSpawnedQueen()
	CancelTimer(25090)
	If Alias_Queen == None || !IsRunning() || !IsStageDone(900) || IsEventOver()
		Return
	EndIf

	Actor queenRef = Alias_Queen.GetActorReference()
	If queenRef == None
		Quest owner = Self as Quest
		DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
		If waveScript == None
			Return
		EndIf
		Int queenWave = waveScript.FindEncounterWaveIndex("Queen (spawned on failure)")
		If queenWave < 0
			Return
		EndIf
		RefCollectionAlias queenCollection = waveScript.EncounterWaves[queenWave].BossRefCollection
		Int index = 0
		While queenCollection != None && queenRef == None && index < queenCollection.GetCount()
			queenRef = queenCollection.GetAt(index) as Actor
			index += 1
		EndWhile
		If queenRef == None
			; The shared wave script defers a subwave by timer while another wave is mid-spawn.
			If waveScript.IsEncounterWaveSpawning(queenWave)
				StartTimer(2.0, 25090)
			EndIf
			Return
		EndIf
		; Alias 8 carries the queen's travel package, the objective 900 marker and the OnDying stage 1001 script.
		Alias_Queen.ForceRefTo(queenRef)
	EndIf

	If queenRef.IsDead() && !IsStageDone(1001)
		SetStage(1001)
	EndIf
EndFunction

Function SpawnQueen()
	If IsEventOver()
		Return
	EndIf
	FailOpenWaveObjectives()
	SetObjectiveDisplayed(900, True, True)
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StopAllEncounterWaves(False)
		waveScript.StartEncounterWaveByID("Queen (spawned on failure)")
	EndIf
	BindSpawnedQueen()
EndFunction

Function GrantEventRewardSpell()
	Actor playerRef = Game.GetPlayer()
	If TWZ05RewardSpell == None || playerRef == None
		Return
	EndIf
	Quest owner = Self as Quest
	DefaultEventQuest eventQuest = owner as DefaultEventQuest
	If eventQuest != None && !eventQuest.IsPlayerParticipating()
		Return
	EndIf
	TWZ05RewardSpell.Cast(playerRef, playerRef)
EndFunction

Function ShutdownEvent(Bool abFailed)
	CancelTimer(25090)
	If abFailed
		FailOpenObjective(100)
		FailOpenWaveObjectives()
		FailOpenObjective(900)
	EndIf
	Stop()
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 25090
		BindSpawnedQueen()
	EndIf
EndEvent

Function Fragment_Stage_0100_Item_00()
	If !IsStageDone(150)
		ResetEventObjectives()
	EndIf
	SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
	CompleteOpenObjective(100)
	StartWavePhase(101, 160, "Wave 1 - Part 1")
EndFunction

Function Fragment_Stage_0175_Item_00()
	StartLeaderSubwave(165, "Wave 1 - Part 2", "Wave 1 Leader: Crab")
EndFunction

Function Fragment_Stage_0200_Item_00()
	CompleteOpenObjective(165)
	FinishWaveIfDefeated(200, 300, 101, 170, -1, 315)
EndFunction

Function Fragment_Stage_0300_Item_00()
	CompleteOpenObjective(160)
	FinishWaveIfDefeated(200, 300, 101, 170, -1, 315)
EndFunction

Function Fragment_Stage_0310_Item_00()
	BeginLastChance(101, 170, 315)
EndFunction

Function Fragment_Stage_0315_Item_00()
	BeginDowntime(180)
EndFunction

Function Fragment_Stage_0325_Item_00()
	EndDowntime(180, 350)
EndFunction

Function Fragment_Stage_0350_Item_00()
	StartWavePhase(300, 360, "Wave 2 - Part 1")
EndFunction

Function Fragment_Stage_0375_Item_00()
	StartLeaderSubwave(365, "Wave 2 - Part 2", "Wave 2 Leader: Hunter")
EndFunction

Function Fragment_Stage_0400_Item_00()
	CompleteOpenObjective(365)
	FinishWaveIfDefeated(400, 500, 300, 370, -1, 515)
EndFunction

Function Fragment_Stage_0500_Item_00()
	CompleteOpenObjective(360)
	FinishWaveIfDefeated(400, 500, 300, 370, -1, 515)
EndFunction

Function Fragment_Stage_0510_Item_00()
	BeginLastChance(300, 370, 515)
EndFunction

Function Fragment_Stage_0515_Item_00()
	BeginDowntime(380)
EndFunction

Function Fragment_Stage_0525_Item_00()
	EndDowntime(380, 550)
EndFunction

Function Fragment_Stage_0550_Item_00()
	StartWavePhase(500, 560, "Wave 3 - Part 1")
EndFunction

Function Fragment_Stage_0575_Item_00()
	StartLeaderSubwave(565, "Wave 3 - Part 2", "Wave 3 Leader: King")
EndFunction

Function Fragment_Stage_0600_Item_00()
	CompleteOpenObjective(565)
	FinishWaveIfDefeated(600, 700, 500, 570, 50, 1000)
EndFunction

Function Fragment_Stage_0700_Item_00()
	CompleteOpenObjective(560)
	FinishWaveIfDefeated(600, 700, 500, 570, 50, 1000)
EndFunction

Function Fragment_Stage_0750_Item_00()
	BeginLastChance(500, 570, 1000)
EndFunction

Function Fragment_Stage_0900_Item_00()
	SpawnQueen()
EndFunction

Function Fragment_Stage_0999_Item_00()
	ShutdownEvent(True)
EndFunction

Function Fragment_Stage_1000_Item_00()
	GrantEventRewardSpell()
	ShutdownEvent(False)
EndFunction

Function Fragment_Stage_1001_Item_00()
	CompleteOpenObjective(900)
	ShutdownEvent(True)
EndFunction

Function Fragment_Stage_9991_Item_00()
	ShutdownEvent(True)
EndFunction
