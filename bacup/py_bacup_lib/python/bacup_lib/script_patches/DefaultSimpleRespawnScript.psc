Bool Function CanSpawnActors()
	If !AllowSpawning || !IsRunning()
		Return False
	EndIf
	If SpawnActorsStage >= 0 && !IsStageDone(SpawnActorsStage)
		Return False
	EndIf
	If StopSpawningStage >= 0 && IsStageDone(StopSpawningStage)
		Return False
	EndIf
	Return True
EndFunction

Function RemoveDeadActors()
	If RespawnCollection == None
		Return
	EndIf

	Int index = RespawnCollection.GetCount() - 1
	While index >= 0
		ObjectReference spawnedRef = RespawnCollection.GetAt(index)
		Actor spawnedActor = spawnedRef as Actor
		If spawnedRef == None || (spawnedActor != None && spawnedActor.IsDead())
			If spawnedRef != None
				spawnedRef.Disable(False)
				spawnedRef.Delete()
				RespawnCollection.RemoveRef(spawnedRef)
			EndIf
		EndIf
		index -= 1
	EndWhile
EndFunction

Bool Function SpawnMissingActors()
	If respawnLock || !CanSpawnActors() || RespawnCollection == None || SpawnArea == None
		Return False
	EndIf
	If ActorsToSpawn == None || ActorsToSpawn.Length == 0
		Return False
	EndIf

	ObjectReference spawnMarker = SpawnArea.GetReference()
	If spawnMarker == None
		Return False
	EndIf

	respawnLock = True
	RemoveDeadActors()

	Int desiredCount = NumActorsToSpawn
	If desiredCount < 1
		desiredCount = ActorsToSpawn.Length
	EndIf
	Int currentCount = RespawnCollection.GetCount()
	Int attemptCount = 0
	Bool spawnedAny = False
	While currentCount < desiredCount && attemptCount < desiredCount
		ActorBase actorToSpawn = ActorsToSpawn[attemptCount % ActorsToSpawn.Length]
		If actorToSpawn != None
			Actor spawnedActor = spawnMarker.PlaceAtMe(actorToSpawn, 1, True, False, True) as Actor
			If spawnedActor != None
				RespawnCollection.AddRef(spawnedActor)
				currentCount += 1
				spawnedAny = True
			EndIf
		EndIf
		attemptCount += 1
	EndWhile

	ActorsSpawned = currentCount > 0
	respawnLock = False
	Return spawnedAny
EndFunction

Function ArmRespawnTimer()
	Float respawnDelay = RespawnTimerSecondsMin as Float
	If respawnDelay < 1.0
		respawnDelay = 1.0
	EndIf
	CancelTimer(RespawnTimerID)
	StartTimer(respawnDelay, RespawnTimerID)
EndFunction

Function BeginSpawning()
	If RespawnCollection == None || SpawnArea == None || ActorsToSpawn == None || ActorsToSpawn.Length == 0
		AllowSpawning = False
		Return
	EndIf

	AllowSpawning = True
	respawnLock = False
	ActorsSpawned = False
	RespawnTimerResetCount = 0
	SpawnMissingActors()
	ArmRespawnTimer()
EndFunction

Function StopSpawning(Bool abCleanUp = True)
	AllowSpawning = False
	CancelTimer(RespawnTimerID)
	respawnLock = False
	If abCleanUp && RespawnCollection != None
		Int index = RespawnCollection.GetCount() - 1
		While index >= 0
			ObjectReference spawnedRef = RespawnCollection.GetAt(index)
			If spawnedRef != None
				spawnedRef.Disable(False)
				spawnedRef.Delete()
			EndIf
			index -= 1
		EndWhile
		RespawnCollection.RemoveAll()
	EndIf
	ActorsSpawned = False
EndFunction

Event OnQuestInit()
	AllowSpawning = True
	respawnLock = False
	ActorsSpawned = False
	RespawnTimerResetCount = 0
	If SpawnActorsStage < 0 || IsStageDone(SpawnActorsStage)
		BeginSpawning()
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == SpawnActorsStage
		BeginSpawning()
	ElseIf auiStageID == StopSpawningStage
		StopSpawning()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != RespawnTimerID
		Return
	EndIf
	If !CanSpawnActors()
		StopSpawning()
		Return
	EndIf

	Bool spawnedAny = SpawnMissingActors()
	If spawnedAny
		RespawnTimerResetCount += 1
	EndIf
	If RespawnTimerMaxResetCount < 0 || RespawnTimerResetCount < RespawnTimerMaxResetCount
		ArmRespawnTimer()
	EndIf
EndEvent

Event OnQuestShutdown()
	StopSpawning()
EndEvent
