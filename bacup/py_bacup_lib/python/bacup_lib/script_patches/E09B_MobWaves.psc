Event OnQuestInit()
	QuestShuttingDown = False
	initialized = True
	B21WaveActors = New Actor[0]
	B21WaveActorWaves = New Int[0]
	B21WaveActive = False
	B21WaveSpawning = False
	B21ActiveWave = -1
	B21ActiveSubwaves = 0
	B21ActiveSpawned = 0
	B21ActiveTimeRemaining = 0.0
EndEvent

Event OnQuestShutdown()
	QuestShuttingDown = True
	StopWave(True)
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 8950
		SpawnSubwave()
	EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
	UnregisterForRemoteEvent(akSender, "OnDeath")
	If B21WaveActors == None
		Return
	EndIf
	Int row = B21WaveActors.Find(akSender)
	If row < 0
		Return
	EndIf
	Int waveIndex = B21WaveActorWaves[row]
	If IsValidWave(waveIndex)
		EncounterWaveData waveData = EncounterWaves[waveIndex]
		If waveData.RemoveActorsFromRefCollectionsOnDeath && waveData.WaveRefCollection != None
			waveData.WaveRefCollection.RemoveRef(akSender)
		EndIf
	EndIf
	B21WaveActors.Remove(row)
	B21WaveActorWaves.Remove(row)
	; Delete waits for the cell to unload, so the corpse stays lootable.
	akSender.Delete()
	EvaluateWaveEnd(waveIndex)
EndEvent

Bool Function IsValidWave(Int aiWaveIndex)
	Return EncounterWaves != None && aiWaveIndex >= 0 && aiWaveIndex < EncounterWaves.Length && EncounterWaves[aiWaveIndex] != None
EndFunction

Int Function FindWaveIndex(String asIDString)
	If EncounterWaves == None || asIDString == ""
		Return -1
	EndIf
	Return EncounterWaves.FindStruct("IDString", asIDString)
EndFunction

Location Function WaveNameLocation(Int aiWaveIndex)
	If !IsValidWave(aiWaveIndex)
		Return None
	EndIf
	Return EncounterWaves[aiWaveIndex].WaveNameLocationForm
EndFunction

Bool Function StartWave(Int aiWaveIndex)
	If QuestShuttingDown || !IsValidWave(aiWaveIndex)
		Return False
	EndIf
	StopWave(False)
	If B21WaveActors == None
		B21WaveActors = New Actor[0]
		B21WaveActorWaves = New Int[0]
	EndIf
	B21ActiveWave = aiWaveIndex
	B21ActiveSubwaves = 0
	B21ActiveSpawned = 0
	B21ActiveTimeRemaining = EncounterWaves[aiWaveIndex].WaveTimeSeconds
	B21WaveActive = True
	SpawnSubwave()
	Return True
EndFunction

Function StartWaveByID(String asIDString)
	StartWave(FindWaveIndex(asIDString))
EndFunction

Function StopWave(Bool abRemoveActors)
	CancelTimer(8950)
	B21WaveActive = False
	If abRemoveActors
		RemoveWaveActors()
	EndIf
EndFunction

Bool Function IsWaveSpawning()
	Return B21WaveActive
EndFunction

Int Function CountLivingWaveActors()
	Int living = 0
	Int index = 0
	While B21WaveActors != None && index < B21WaveActors.Length
		Actor waveActor = B21WaveActors[index]
		If waveActor != None && !waveActor.IsDead() && !waveActor.IsDisabled()
			living += 1
		EndIf
		index += 1
	EndWhile
	Return living
EndFunction

Int Function CountLivingActorsNotIn(ObjectReference[] akRefs)
	Int remaining = 0
	Int index = 0
	While B21WaveActors != None && index < B21WaveActors.Length
		Actor waveActor = B21WaveActors[index]
		If waveActor != None && !waveActor.IsDead() && !waveActor.IsDisabled()
			If akRefs == None || akRefs.Find(waveActor) < 0
				remaining += 1
			EndIf
		EndIf
		index += 1
	EndWhile
	Return remaining
EndFunction

Float Function SubwaveIntervalSeconds(EncounterWaveData akWave)
	; CT_EWS_SubwaveTime_* curves (the Short variants hold the same values).
	Float seconds = 60.0
	If akWave.SubwaveTimer_Speed == 0
		seconds = 120.0
	ElseIf akWave.SubwaveTimer_Speed == 1
		seconds = 90.0
	ElseIf akWave.SubwaveTimer_Speed == 3
		seconds = 30.0
	ElseIf akWave.SubwaveTimer_Speed == 4
		seconds = 15.0
	ElseIf akWave.SubwaveTimer_Speed == 5
		seconds = 5.0
	ElseIf akWave.SubwaveTimer_Speed == 6
		seconds = 0.0
	EndIf
	If akWave.SubwaveDelayMult > 0.0
		seconds *= akWave.SubwaveDelayMult
	EndIf
	If seconds < 1.0
		seconds = 1.0
	EndIf
	Return seconds
EndFunction

Int Function SubwaveActorCount(EncounterWaveData akWave)
	Int count = akWave.Difficulty + 1
	If akWave.Difficulty < 0 || akWave.Difficulty > 4
		count = 3
	EndIf
	If akWave.MaxSubwaveActorsOverride > 0 && count > akWave.MaxSubwaveActorsOverride
		count = akWave.MaxSubwaveActorsOverride
	EndIf
	If akWave.MinSubwaveActorsOverride > 1 && count < akWave.MinSubwaveActorsOverride
		count = akWave.MinSubwaveActorsOverride
	EndIf
	Return count
EndFunction

Int Function WaveLiveActorCap(EncounterWaveData akWave)
	If akWave.MaxWaveActorsOverride > 0
		Return akWave.MaxWaveActorsOverride
	EndIf
	Return 5
EndFunction

Int Function WaveDurationLimit(EncounterWaveData akWave)
	Int limit = akWave.WaveDuration
	If limit < 1
		limit = 1
	ElseIf limit > 99
		limit = 99
	EndIf
	Return limit
EndFunction

ObjectReference Function ResolveWaveSpawnPoint(EncounterWaveData akWave)
	If akWave.SpawnArea != None && akWave.SpawnArea.GetReference() != None
		Return akWave.SpawnArea.GetReference()
	EndIf
	Quest owner = Self as Quest
	DefaultEventQuest eventQuest = owner as DefaultEventQuest
	If eventQuest != None && eventQuest.CenterMarker != None
		Return eventQuest.CenterMarker.GetReference()
	EndIf
	Return None
EndFunction

Function SpawnSubwave()
	If QuestShuttingDown || !B21WaveActive || B21WaveSpawning || !IsValidWave(B21ActiveWave)
		Return
	EndIf
	B21WaveSpawning = True

	Int waveIndex = B21ActiveWave
	EncounterWaveData waveData = EncounterWaves[waveIndex]
	Quest owner = Self as Quest
	B21:EncounterWaveCatalog catalog = owner as B21:EncounterWaveCatalog
	ObjectReference spawnPoint = ResolveWaveSpawnPoint(waveData)
	Int variants = 0
	If catalog != None
		variants = catalog.VariantCount(waveIndex)
	EndIf
	Int spawnCount = SubwaveActorCount(waveData)
	Int capacity = WaveLiveActorCap(waveData) - CountLivingWaveActors()
	If spawnCount > capacity
		spawnCount = capacity
	EndIf
	If waveData.WaveType == 3 && spawnCount > WaveDurationLimit(waveData) - B21ActiveSpawned
		spawnCount = WaveDurationLimit(waveData) - B21ActiveSpawned
	EndIf

	Int spawned = 0
	If spawnPoint != None && variants > 0 && spawnCount > 0
		Int variant = Utility.RandomInt(0, variants - 1)
		Int actorIndex = 0
		While actorIndex < spawnCount && B21WaveActive && !QuestShuttingDown
			Form spawnForm = catalog.PickCandidate(waveIndex, variant, actorIndex)
			If spawnForm != None
				Actor waveActor = spawnPoint.PlaceAtMe(spawnForm, 1, False, True, False) as Actor
				If waveActor != None
					AddWaveActor(waveIndex, waveData, waveActor)
					spawned += 1
				EndIf
			EndIf
			actorIndex += 1
		EndWhile
	EndIf
	B21WaveSpawning = False
	If QuestShuttingDown || !B21WaveActive || waveIndex != B21ActiveWave
		Return
	EndIf

	B21ActiveSpawned += spawned
	B21ActiveSubwaves += 1
	ScheduleNextSubwave(waveData)
EndFunction

Function ScheduleNextSubwave(EncounterWaveData akWave)
	Float interval = SubwaveIntervalSeconds(akWave)
	Bool finished = False
	If akWave.BossWave
		finished = True
	ElseIf akWave.WaveType == 1
		B21ActiveTimeRemaining -= interval
		finished = B21ActiveTimeRemaining <= 0.0
	ElseIf akWave.WaveType == 2
		finished = B21ActiveSubwaves >= WaveDurationLimit(akWave)
	ElseIf akWave.WaveType == 3
		finished = B21ActiveSpawned >= WaveDurationLimit(akWave)
	ElseIf akWave.WaveType != 0
		finished = True
	EndIf

	If !finished
		StartTimer(interval, 8950)
		Return
	EndIf
	B21WaveActive = False
	EvaluateWaveEnd(B21ActiveWave)
EndFunction

Function AddWaveActor(Int aiWaveIndex, EncounterWaveData akWave, Actor akActor)
	B21WaveActors.Add(akActor)
	B21WaveActorWaves.Add(aiWaveIndex)
	If akWave.WaveRefCollection != None
		akWave.WaveRefCollection.AddRef(akActor)
	EndIf
	RegisterForRemoteEvent(akActor, "OnDeath")
	akActor.EnableNoWait()

	Quest owner = Self as Quest
	E09B_Script wheelScript = owner as E09B_Script
	If wheelScript != None
		wheelScript.HandleWaveActorAdded(akActor, aiWaveIndex)
	EndIf
	Quests:_Default:SetPreferredCombatTargets preferred = akWave.WaveRefCollection as Quests:_Default:SetPreferredCombatTargets
	If preferred != None
		preferred.ApplyPreferredCombatTarget(akActor)
	EndIf
EndFunction

Function EvaluateWaveEnd(Int aiWaveIndex)
	If QuestShuttingDown || B21WaveActive || aiWaveIndex != B21ActiveWave || !IsValidWave(aiWaveIndex)
		Return
	EndIf
	; Without catalog candidates nothing spawned; the round's objective timer ends it instead of an instant clear.
	If B21ActiveSpawned <= 0 || CountLivingWaveActors() > 0
		Return
	EndIf
	B21ActiveSpawned = 0
	Int stage = EncounterWaves[aiWaveIndex].StageToSetAtEnd
	If stage >= 0
		SetStage(stage)
	EndIf
EndFunction

Function RemoveWaveActors()
	Int index = 0
	While B21WaveActors != None && index < B21WaveActors.Length
		Actor waveActor = B21WaveActors[index]
		If waveActor != None
			UnregisterForRemoteEvent(waveActor, "OnDeath")
			Int waveIndex = B21WaveActorWaves[index]
			If IsValidWave(waveIndex) && EncounterWaves[waveIndex].WaveRefCollection != None
				EncounterWaves[waveIndex].WaveRefCollection.RemoveRef(waveActor)
			EndIf
			If !waveActor.IsDead()
				waveActor.DisableNoWait()
			EndIf
			waveActor.Delete()
		EndIf
		index += 1
	EndWhile
	B21WaveActors = New Actor[0]
	B21WaveActorWaves = New Int[0]
EndFunction
