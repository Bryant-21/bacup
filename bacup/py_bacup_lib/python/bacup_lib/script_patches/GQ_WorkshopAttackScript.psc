Event OnQuestInit()
	currentWaveIndex = -1
	progressPercentage = 0.0
	WaveCount = AvailableWaveCount()
	PublishWaveCount()
	ResetAttackObjectives()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	If startAttackStage >= 0 && !IsStageDone(startAttackStage)
		SetStage(startAttackStage)
	EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	; StartTimer timers do not survive a reload, so every open phase re-arms here.
	If akSender != Game.GetPlayer() || !IsRunning()
		Return
	EndIf
	If IsStageDone(completionStage) || IsStageDone(AttackersWinStage)
		Return
	EndIf
	If firstSpawnStage >= 0 && IsStageDone(firstSpawnStage)
		ArmAttackersWinTimer()
		StartTimer(3.0, 102)
	ElseIf startAttackStage >= 0 && IsStageDone(startAttackStage)
		StartTimer(120.0, 104)
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(AttackersWinTimerID)
	CancelTimer(102)
	CancelTimer(103)
	CancelTimer(104)
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	DefaultQuestEncounterWaveScript waveScript = EncounterWaveScript()
	If waveScript != None
		waveScript.StopAllEncounterWaves(True)
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == firstSpawnStage
		BeginWave(0)
		Return
	EndIf
	Int finishedWave = WaveIndexForEndStage(auiStageID)
	If finishedWave >= 0
		OnWaveFinished(finishedWave)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == AttackersWinTimerID
		If !IsRunning() || IsStageDone(completionStage) || IsStageDone(AttackersWinStage)
			Return
		EndIf
		If PlayerHasAbandonedDefense()
			SetStage(AttackersWinStage)
		Else
			ArmAttackersWinTimer()
		EndIf
	ElseIf aiTimerID == 102
		If !IsRunning() || IsStageDone(completionStage) || IsStageDone(AttackersWinStage)
			Return
		EndIf
		NameAttackers()
		StartTimer(3.0, 102)
	ElseIf aiTimerID == 103
		If IsRunning()
			Stop()
		EndIf
	ElseIf aiTimerID == 104
		; Watchdog for the converted prepare-phase objective timer.
		If IsRunning() && !IsStageDone(AttackersWinStage) && !IsStageDone(firstSpawnStage)
			BeginAttack()
		EndIf
	EndIf
EndEvent

DefaultQuestEncounterWaveScript Function EncounterWaveScript()
	Quest owner = Self as Quest
	Return owner as DefaultQuestEncounterWaveScript
EndFunction

DefaultEventQuest Function EventQuestScript()
	Quest owner = Self as Quest
	Return owner as DefaultEventQuest
EndFunction

Int Function AvailableWaveCount()
	Int authoredWaves = 0
	If Waves != None
		authoredWaves = Waves.Length
	EndIf
	DefaultQuestEncounterWaveScript waveScript = EncounterWaveScript()
	If waveScript != None && waveScript.EncounterWaves != None && waveScript.EncounterWaves.Length < authoredWaves
		authoredWaves = waveScript.EncounterWaves.Length
	EndIf
	If !allowBossWave && authoredWaves > 1
		authoredWaves -= 1
	EndIf
	Return authoredWaves
EndFunction

Function PublishWaveCount()
	Quest owner = Self as Quest
	B21:QuestVariables questVariables = owner as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable(waveCountName, WaveCount as Float)
	EndIf
EndFunction

Int Function WaveObjectiveIndex(Int aiWaveIndex)
	; Objectives 10/20/30/40/50 for waves 1..5, from swarmObjectiveIndex.
	Return swarmObjectiveIndex * (aiWaveIndex + 1)
EndFunction

Function ResetAttackObjectives()
	SetObjectiveDisplayed(5, False)
	SetObjectiveCompleted(5, False)
	SetObjectiveFailed(5, False)
	Int waveIndex = 0
	Int availableWaves = AvailableWaveCount()
	While waveIndex < availableWaves
		Int objectiveIndex = WaveObjectiveIndex(waveIndex)
		SetObjectiveDisplayed(objectiveIndex, False)
		SetObjectiveCompleted(objectiveIndex, False)
		SetObjectiveFailed(objectiveIndex, False)
		waveIndex += 1
	EndWhile
EndFunction

Function BeginPrepare()
	WaveCount = AvailableWaveCount()
	PublishWaveCount()
	SetObjectiveDisplayed(5, True, True)
	StartTimer(120.0, 104)
EndFunction

Function BeginAttack()
	CancelTimer(104)
	If !IsObjectiveCompleted(5)
		SetObjectiveCompleted(5, True)
	EndIf
	If firstSpawnStage >= 0 && !IsStageDone(firstSpawnStage)
		SetStage(firstSpawnStage)
	EndIf
EndFunction

Function BeginWave(Int aiWaveIndex)
	If aiWaveIndex < 0 || aiWaveIndex >= WaveCount || IsStageDone(completionStage) || IsStageDone(AttackersWinStage)
		Return
	EndIf
	If currentWaveIndex == aiWaveIndex
		Return
	EndIf
	currentWaveIndex = aiWaveIndex
	SetObjectiveDisplayed(WaveObjectiveIndex(aiWaveIndex), True, True)
	AddParticipantToWaveRewards(aiWaveIndex)
	DefaultQuestEncounterWaveScript waveScript = EncounterWaveScript()
	If waveScript != None
		waveScript.StartEncounterWave(aiWaveIndex)
	EndIf
	NameAttackers()
	StartTimer(3.0, 102)
	ArmAttackersWinTimer()
EndFunction

Function AddParticipantToWaveRewards(Int aiWaveIndex)
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || Waves == None || aiWaveIndex >= Waves.Length
		Return
	EndIf
	DefaultEventQuest eventQuest = EventQuestScript()
	If eventQuest != None && !eventQuest.IsPlayerParticipating()
		Return
	EndIf
	RefCollectionAlias rewardCollection = Waves[aiWaveIndex].rewardPlayers
	If rewardCollection != None && rewardCollection.Find(playerRef) < 0
		rewardCollection.AddRef(playerRef)
	EndIf
EndFunction

Int Function WaveIndexForEndStage(Int aiStage)
	If aiStage < 0
		Return -1
	EndIf
	DefaultQuestEncounterWaveScript waveScript = EncounterWaveScript()
	Int waveIndex = 0
	While waveScript != None && waveScript.EncounterWaves != None && waveIndex < waveScript.EncounterWaves.Length
		If waveScript.EncounterWaves[waveIndex].StageToSetAtEnd == aiStage
			Return waveIndex
		EndIf
		waveIndex += 1
	EndWhile
	; The authored wave-dead stages follow the first-wave stage one per wave
	; (20 -> 21..25), which keeps the chain moving if the wave rows read empty.
	If firstSpawnStage >= 0 && aiStage > firstSpawnStage && aiStage <= firstSpawnStage + WaveCount
		Return aiStage - firstSpawnStage - 1
	EndIf
	Return -1
EndFunction

Function OnWaveFinished(Int aiWaveIndex)
	If aiWaveIndex < 0 || IsStageDone(completionStage) || IsStageDone(AttackersWinStage)
		Return
	EndIf
	Int objectiveIndex = WaveObjectiveIndex(aiWaveIndex)
	If IsObjectiveDisplayed(objectiveIndex) && !IsObjectiveCompleted(objectiveIndex)
		SetObjectiveCompleted(objectiveIndex, True)
	EndIf
	If WaveCount > 0
		progressPercentage = (aiWaveIndex + 1) as Float / WaveCount as Float
	EndIf
	If aiWaveIndex + 1 >= WaveCount
		CancelTimer(AttackersWinTimerID)
		If completionStage >= 0 && !IsStageDone(completionStage)
			SetStage(completionStage)
		EndIf
	Else
		BeginWave(aiWaveIndex + 1)
	EndIf
EndFunction

Function NameAttackers()
	; <Alias.RacePlural=AttackerName> needs a living attacker in the name alias.
	If Alias_SwarmName == None || Alias_SwarmName.GetReference() != None
		Return
	EndIf
	Actor attacker = FirstLivingAttacker()
	If attacker != None
		Alias_SwarmName.ForceRefTo(attacker)
	EndIf
EndFunction

Actor Function FirstLivingAttacker()
	If Waves == None || currentWaveIndex < 0 || currentWaveIndex >= Waves.Length
		Return None
	EndIf
	RefCollectionAlias attackers = Waves[currentWaveIndex].enemies
	If attackers == None
		Return None
	EndIf
	Int index = 0
	While index < attackers.GetCount()
		Actor attacker = attackers.GetAt(index) as Actor
		If attacker != None && !attacker.IsDead()
			Return attacker
		EndIf
		index += 1
	EndWhile
	Return None
EndFunction

Function ArmAttackersWinTimer()
	CancelTimer(AttackersWinTimerID)
	Float window = AttackersWinTimerSecondsPerWave
	If window < 1.0
		window = 1.0
	EndIf
	StartTimer(window, AttackersWinTimerID)
EndFunction

Bool Function PlayerHasAbandonedDefense()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return False
	EndIf
	ObjectReference centerRef = None
	If Alias_CenterMarker != None
		centerRef = Alias_CenterMarker.GetReference()
	EndIf
	If centerRef == None
		centerRef = WorkshopReference()
	EndIf
	If centerRef == None
		Return False
	EndIf
	Return playerRef.GetDistance(centerRef) > AttackersWinMinPlayerDistance
EndFunction

ObjectReference Function WorkshopReference()
	If Alias_Workshop == None
		Return None
	EndIf
	Return Alias_Workshop.GetReference()
EndFunction

Location Function AttackLocationValue()
	Quest owner = Self as Quest
	LocationAlias attackLocation = owner.GetAlias(0) as LocationAlias
	If attackLocation != None && attackLocation.GetLocation() != None
		Return attackLocation.GetLocation()
	EndIf
	ObjectReference workshopRef = WorkshopReference()
	If workshopRef != None
		Return workshopRef.GetCurrentLocation()
	EndIf
	Return None
EndFunction

Function CloseWaveObjectives(Bool abFailed)
	If IsObjectiveDisplayed(5) && !IsObjectiveCompleted(5)
		If abFailed
			SetObjectiveFailed(5, True)
		Else
			SetObjectiveCompleted(5, True)
		EndIf
	EndIf
	Int waveIndex = 0
	While waveIndex < WaveCount
		Int objectiveIndex = WaveObjectiveIndex(waveIndex)
		If IsObjectiveDisplayed(objectiveIndex) && !IsObjectiveCompleted(objectiveIndex)
			If abFailed
				SetObjectiveFailed(objectiveIndex, True)
			Else
				SetObjectiveCompleted(objectiveIndex, True)
			EndIf
		EndIf
		waveIndex += 1
	EndWhile
EndFunction

Function StartTakeoverQuest()
	ObjectReference workshopRef = WorkshopReference()
	Bool startedTakeover = False
	If WorkshopEventAttack_Takeover != None
		startedTakeover = WorkshopEventAttack_Takeover.SendStoryEventAndWait(AttackLocationValue(), workshopRef)
	EndIf
	If startedTakeover
		Return
	EndIf
	; No converted Story Manager node consumes WorkshopEventAttack_Takeover, so the
	; takeover quest (which the converter left event-scope free) is started directly.
	Quest takeoverQuest = Game.GetFormFromFile(0x00009179, "SeventySix.esm") as Quest
	If takeoverQuest == None || takeoverQuest.IsRunning()
		Return
	EndIf
	If takeoverQuest.IsCompleted() || takeoverQuest.IsStopped()
		takeoverQuest.Reset()
	EndIf
	ReferenceAlias takeoverWorkshop = takeoverQuest.GetAlias(1) as ReferenceAlias
	If takeoverWorkshop != None && workshopRef != None
		takeoverWorkshop.ForceRefTo(workshopRef)
	EndIf
	If !takeoverQuest.Start()
		Debug.Trace("[B21 Workshop] Retake quest 009179 refused to start (alias fill); workshop=" + workshopRef as String, 0)
		Return
	EndIf
	If takeoverWorkshop != None && workshopRef != None && takeoverWorkshop.GetReference() == None
		takeoverWorkshop.ForceRefTo(workshopRef)
	EndIf
EndFunction

Function ScheduleShutdown()
	CancelTimer(103)
	StartTimer(10.0, 103)
EndFunction

Function ShutdownAttack(Bool abFailed)
	CancelTimer(AttackersWinTimerID)
	CancelTimer(102)
	CancelTimer(104)
	CloseWaveObjectives(abFailed)
	DefaultQuestEncounterWaveScript waveScript = EncounterWaveScript()
	If waveScript != None
		; A lost defence leaves its attackers holding the workshop for the takeover.
		waveScript.StopAllEncounterWaves(!abFailed)
	EndIf
	If abFailed
		progressPercentage = 0.0
		StartTakeoverQuest()
	Else
		progressPercentage = 1.0
	EndIf
	ScheduleShutdown()
EndFunction
