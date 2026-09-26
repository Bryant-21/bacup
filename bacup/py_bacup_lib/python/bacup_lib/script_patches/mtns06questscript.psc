Event OnQuestInit()
	ResetActivityState()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == WaveActiveTimerID
		If bActivityIsActive && !IsStageDone(200)
			SetStage(200)
		EndIf
	ElseIf aiTimerID == BossTimerId
		StartNextBoss()
	ElseIf aiTimerID == 4
		EndActivity()
	ElseIf aiTimerID == 5
		If IsRunning() && !IsStageDone(DoneStage)
			SetStage(DoneStage)
		EndIf
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(WaveActiveTimerID)
	CancelTimer(BossTimerId)
	CancelTimer(4)
	CancelTimer(5)
	bActivityIsActive = False
	StopExtractors()
	If MTNS06_Uranium_Misc != None && MTNS06_Uranium_Misc.IsRunning() && !MTNS06_Uranium_Misc.IsStageDone(200)
		MTNS06_Uranium_Misc.SetStage(200)
	EndIf
EndEvent

Function ResetActivityState()
	bActivityIsActive = False
	AddUraniumSpinLock = False
	EndlessWavePaused = False
	iBoss = 0
	iUraniumTotal = 0
	UraniumTotalProgress = 0.0
	RewardTier01Unlocked = False
	RewardTier02Unlocked = False
	RewardTier03Unlocked = False
	RewardTier04Unlocked = False
	ReadRewardThresholds()
	PublishRewardTier(0)
EndFunction

Float Function GlobalOrDefault(GlobalVariable akGlobal, Float afDefault)
	If akGlobal == None
		Return afDefault
	EndIf
	Return akGlobal.GetValue()
EndFunction

Function ReadRewardThresholds()
	fRewardPointsTier01 = GlobalOrDefault(MTNS06_RewardPointsTier01, 450.0)
	fRewardPointsTier02 = GlobalOrDefault(MTNS06_RewardPointsTier02, 750.0)
	fRewardPointsTier03 = GlobalOrDefault(MTNS06_RewardPointsTier03, 1000.0)
	fRewardPointsTier04 = GlobalOrDefault(MTNS06_RewardPointsTier04, 1200.0)
	iUraniumTotalMax = fRewardPointsTier04 as Int
EndFunction

Float Function ActivitySeconds()
	Float seconds = GlobalOrDefault(MTNS06_ActivityTimer, 480.0)
	If seconds < 1.0
		seconds = 1.0
	EndIf
	Return seconds
EndFunction

Float Function UraniumPerExtractorSecond()
	Int extractorCount = 0
	If ExtractorAliasData != None
		extractorCount = ExtractorAliasData.Length
	EndIf
	; Observed FO76 tuning: an uninterrupted run reaches the top tier with 40 seconds of total extractor downtime to spare.
	Float productiveSeconds = ActivitySeconds() * extractorCount - 40.0
	If productiveSeconds <= 0.0
		Return 1.0
	EndIf
	Return fRewardPointsTier04 / productiveSeconds
EndFunction

Int Function CurrentRewardTier()
	If UraniumTotalProgress >= fRewardPointsTier04
		Return 4
	ElseIf UraniumTotalProgress >= fRewardPointsTier03
		Return 3
	ElseIf UraniumTotalProgress >= fRewardPointsTier02
		Return 2
	ElseIf UraniumTotalProgress >= fRewardPointsTier01
		Return 1
	EndIf
	Return 0
EndFunction

Int Function UnlockedRewardTier()
	If RewardTier04Unlocked
		Return 4
	ElseIf RewardTier03Unlocked
		Return 3
	ElseIf RewardTier02Unlocked
		Return 2
	ElseIf RewardTier01Unlocked
		Return 1
	EndIf
	Return 0
EndFunction

Function PublishRewardTier(Int aiTier)
	Quest owner = Self as Quest
	B21:QuestVariables questVariables = owner as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable(TextVar_RewardTierCurr, aiTier as Float)
		questVariables.SetVariable(TextVar_RewardTierMax, RewardTierMax as Float)
	EndIf
EndFunction

Function AddExtractorUranium(Float afSeconds)
	If !bActivityIsActive || afSeconds <= 0.0
		Return
	EndIf
	UraniumTotalProgress += afSeconds * UraniumPerExtractorSecond()
	iUraniumTotal = UraniumTotalProgress as Int

	Int previousTier = UnlockedRewardTier()
	Int tier = CurrentRewardTier()
	If tier <= previousTier
		Return
	EndIf
	RewardTier01Unlocked = tier >= 1
	RewardTier02Unlocked = tier >= 2
	RewardTier03Unlocked = tier >= 3
	RewardTier04Unlocked = tier >= 4
	PublishRewardTier(tier)
	If MTNS06_UraniumTotalMessage != None
		MTNS06_UraniumTotalMessage.Show(UraniumTotalProgress)
	EndIf
	If MTNS06_Uranium_PA_RewardTierChanged != None && !MTNS06_Uranium_PA_RewardTierChanged.IsPlaying()
		MTNS06_Uranium_PA_RewardTierChanged.Start()
	EndIf
EndFunction

MTNS06_AliasExtractorScript Function ExtractorScriptAt(Int aiIndex)
	If ExtractorAliasData == None || aiIndex < 0 || aiIndex >= ExtractorAliasData.Length || ExtractorAliasData[aiIndex] == None
		Return None
	EndIf
	Return ExtractorAliasData[aiIndex].ExtractorAlias as MTNS06_AliasExtractorScript
EndFunction

Function StartExtractors()
	bInitializingExtractors = True
	Int index = 0
	While ExtractorAliasData != None && index < ExtractorAliasData.Length
		ExtractorAliasDatum row = ExtractorAliasData[index]
		If row != None && row.ExtractorAlias != None
			ObjectReference extractorRef = row.ExtractorAlias.GetReference()
			If extractorRef != None && DefaultAliasOnObjectRepaired.NeedsRepair(extractorRef)
				extractorRef.ClearDestruction()
			EndIf
			If row.DestroyedObjective >= 0
				SetObjectiveDisplayed(row.DestroyedObjective, False)
				SetObjectiveCompleted(row.DestroyedObjective, False)
			EndIf
		EndIf
		MTNS06_AliasExtractorScript extractor = ExtractorScriptAt(index)
		If extractor != None
			extractor.StartExtracting()
		EndIf
		index += 1
	EndWhile
	bInitializingExtractors = False
EndFunction

Function StopExtractors()
	Int index = 0
	While ExtractorAliasData != None && index < ExtractorAliasData.Length
		MTNS06_AliasExtractorScript extractor = ExtractorScriptAt(index)
		If extractor != None
			extractor.StopExtracting()
		EndIf
		index += 1
	EndWhile
EndFunction

Int Function RepairObjectiveFor(ReferenceAlias akExtractorAlias)
	Int index = 0
	While akExtractorAlias != None && ExtractorAliasData != None && index < ExtractorAliasData.Length
		If ExtractorAliasData[index] != None && ExtractorAliasData[index].ExtractorAlias == akExtractorAlias
			Return ExtractorAliasData[index].DestroyedObjective
		EndIf
		index += 1
	EndWhile
	Return -1
EndFunction

Function ExtractorDestroyed(ReferenceAlias akExtractorAlias)
	Int objective = RepairObjectiveFor(akExtractorAlias)
	If !bActivityIsActive || objective < 0
		Return
	EndIf
	SetObjectiveCompleted(objective, False)
	SetObjectiveDisplayed(objective, True, True)
EndFunction

Function ExtractorRepaired(ReferenceAlias akExtractorAlias)
	Int objective = RepairObjectiveFor(akExtractorAlias)
	If objective < 0 || !IsObjectiveDisplayed(objective)
		Return
	EndIf
	If bActivityIsActive
		SetObjectiveCompleted(objective, True)
	Else
		SetObjectiveDisplayed(objective, False)
	EndIf
EndFunction

Function HideOpenRepairObjectives()
	Int index = 0
	While ExtractorAliasData != None && index < ExtractorAliasData.Length
		Int objective = -1
		If ExtractorAliasData[index] != None
			objective = ExtractorAliasData[index].DestroyedObjective
		EndIf
		If objective >= 0 && IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective)
			SetObjectiveDisplayed(objective, False)
		EndIf
		index += 1
	EndWhile
EndFunction

Function StartActivity()
	If bActivityIsActive || !IsRunning() || IsStageDone(ActivityOverSuccessStage) || IsStageDone(ActivityOverFailureStage) || IsStageDone(FailNoActivityStage)
		Return
	EndIf
	ReadRewardThresholds()
	iBoss = 0
	iUraniumTotal = 0
	UraniumTotalProgress = 0.0
	RewardTier01Unlocked = False
	RewardTier02Unlocked = False
	RewardTier03Unlocked = False
	RewardTier04Unlocked = False
	PublishRewardTier(0)
	bActivityIsActive = True
	GoToState("activityisactive")

	StartExtractors()
	Quest owner = Self as Quest
	MTNS06_WaveScript waves = owner as MTNS06_WaveScript
	If waves != None
		waves.StartCreatureAssignment()
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef != None && MTNS06_CameraShakeSpellIntense != None
		MTNS06_CameraShakeSpellIntense.Cast(playerRef, playerRef)
	EndIf
	StartTimer(ActivitySeconds(), 4)
	StartTimer(WaveActiveTimerLength, WaveActiveTimerID)
	StartTimer(BossTimerLength, BossTimerId)
EndFunction

Function StartNextBoss()
	If !bActivityIsActive || iBoss >= 3
		Return
	EndIf
	iBoss += 1
	Int bossStage = BossStage01
	If iBoss == 2
		bossStage = BossStage02
	ElseIf iBoss == 3
		bossStage = BossStage03
	EndIf
	If !IsStageDone(bossStage)
		SetStage(bossStage)
	EndIf
	If iBoss < 3
		StartTimer(BossTimerLength, BossTimerId)
	EndIf
EndFunction

Function StartBossWave(Int aiBossNumber)
	If !bActivityIsActive
		Return
	EndIf
	Quest owner = Self as Quest
	MTNS06_WaveScript waves = owner as MTNS06_WaveScript
	If waves != None
		waves.StartEncounterWaveByID("Boss " + aiBossNumber)
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && MTNS06_CameraShakeSpellIntense != None
		MTNS06_CameraShakeSpellIntense.Cast(playerRef, playerRef)
	EndIf
	SetObjectiveCompleted(300, False)
	SetObjectiveDisplayed(300, True, True)
EndFunction

Int Function CountLivingBosses()
	Int living = 0
	Int index = 0
	While AllBosses != None && index < AllBosses.GetCount()
		Actor boss = AllBosses.GetActorAt(index)
		If boss != None && !boss.IsDead() && !boss.IsDisabled()
			living += 1
		EndIf
		index += 1
	EndWhile
	Return living
EndFunction

Function BossKilled()
	If IsObjectiveDisplayed(300) && !IsObjectiveCompleted(300) && CountLivingBosses() == 0
		SetObjectiveCompleted(300, True)
	EndIf
EndFunction

Function EndActivity()
	If !bActivityIsActive
		Return
	EndIf
	bActivityIsActive = False
	GoToState("activityisnotactive")
	CancelTimer(WaveActiveTimerID)
	CancelTimer(BossTimerId)
	CancelTimer(4)
	StopExtractors()
	If CurrentRewardTier() >= 1
		SetStage(ActivityOverSuccessStage)
	Else
		SetStage(ActivityOverFailureStage)
	EndIf
EndFunction

Function FinishEvent(Bool abSucceeded)
	bActivityIsActive = False
	CancelTimer(WaveActiveTimerID)
	CancelTimer(BossTimerId)
	CancelTimer(4)
	StopExtractors()
	HideOpenRepairObjectives()
	Quest owner = Self as Quest
	MTNS06_WaveScript waves = owner as MTNS06_WaveScript
	If waves != None
		waves.StopCreatureAssignment()
		waves.StopAllEncounterWaves(False)
	EndIf
	If abSucceeded
		SetObjectiveCompleted(ProtectObjective, True)
		SetObjectiveCompleted(ExtractionObjective, True)
	Else
		If IsObjectiveDisplayed(ProtectObjective) && !IsObjectiveCompleted(ProtectObjective)
			SetObjectiveFailed(ProtectObjective, True)
		EndIf
		If IsObjectiveDisplayed(ExtractionObjective) && !IsObjectiveCompleted(ExtractionObjective)
			SetObjectiveFailed(ExtractionObjective, True)
		EndIf
	EndIf
	If IsObjectiveDisplayed(300) && !IsObjectiveCompleted(300)
		SetObjectiveDisplayed(300, False)
	EndIf
	CancelTimer(5)
	StartTimer(60.0, 5)
EndFunction
