Function RegisterStormBossEvents(Actor akBoss)
	If akBoss == None
		Return
	EndIf

	RegisterForAnimationEvent(akBoss, sCallDownLightningStartEvent)
	RegisterForAnimationEvent(akBoss, sCallDownLightningCompleteEvent)
EndFunction

Function UnregisterStormBossEvents(Actor akBoss)
	If akBoss == None
		Return
	EndIf

	UnregisterForAnimationEvent(akBoss, sCallDownLightningStartEvent)
	UnregisterForAnimationEvent(akBoss, sCallDownLightningCompleteEvent)
EndFunction

Function SetMeleeEnabled(Actor akBoss, Bool abEnabled)
	If akBoss != None && !akBoss.IsDead() && Storm_RegionBoss_EnableMeleeAttacks != None
		If abEnabled
			akBoss.SetValue(Storm_RegionBoss_EnableMeleeAttacks, 1.0)
		Else
			akBoss.SetValue(Storm_RegionBoss_EnableMeleeAttacks, 0.0)
		EndIf
	EndIf
EndFunction

Function ClearStrikeMarkers()
	If placedMarkers == None
		Return
	EndIf

	Int index = 0
	While index < placedMarkers.Length
		If placedMarkers[index] != None
			placedMarkers[index].Delete()
		EndIf
		index += 1
	EndWhile
	placedMarkers.Clear()
EndFunction

Function PlaceLocalStrikeMarker(Actor akBoss)
	If akBoss == None || akBoss.IsDead() || akStrikeMarker == None
		Return
	EndIf

	ObjectReference targetRef = akBoss.GetCombatTarget()
	If targetRef == None
		Actor playerRef = Game.GetPlayer()
		If playerRef != None && (fFindPlayerRange <= 0.0 || akBoss.GetDistance(playerRef) <= fFindPlayerRange)
			targetRef = playerRef
		EndIf
	EndIf
	If targetRef == None
		Return
	EndIf

	; No damage form is bound here, so this restores the local telegraph without inventing lightning damage.
	ObjectReference markerRef = targetRef.PlaceAtMe(akStrikeMarker)
	If markerRef != None
		If placedMarkers == None
			placedMarkers = new ObjectReference[0]
		EndIf
		placedMarkers.Add(markerRef)
	EndIf
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	Actor bossRef = akTarget
	If bossRef == None
		bossRef = akCaster
	EndIf
	If bossRef == None || bossRef.IsDead()
		Return
	EndIf

	placedMarkers = new ObjectReference[0]
	RegisterStormBossEvents(bossRef)
	SetMeleeEnabled(bossRef, True)
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	Actor bossRef = GetTargetActor()
	If bossRef == None || bossRef.IsDead() || akSource != bossRef
		Return
	EndIf

	If asEventName == sCallDownLightningStartEvent
		SetMeleeEnabled(bossRef, False)
		ClearStrikeMarkers()
		PlaceLocalStrikeMarker(bossRef)
	ElseIf asEventName == sCallDownLightningCompleteEvent
		ClearStrikeMarkers()
		SetMeleeEnabled(bossRef, True)
	EndIf
EndEvent

Event OnCripple(ActorValue akActorValue, Bool abCrippled)
	Actor bossRef = GetTargetActor()
	If !abCrippled || !bSpawnObjectsOnCripple || bossRef == None || bossRef.IsDead() || CrippleSpawners == None
		Return
	EndIf

	Int index = 0
	While index < CrippleSpawners.Length
		CrippleSpawnObject spawnData = CrippleSpawners[index]
		If spawnData != None && spawnData.CrippleAV == akActorValue && spawnData.ObjectToSpawn != None && spawnData.SpawnCount > 0
			bossRef.PlaceAtNode(spawnData.NodeToSpawnAt, spawnData.ObjectToSpawn, spawnData.SpawnCount)
		EndIf
		index += 1
	EndWhile
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	UnregisterStormBossEvents(akTarget)
	ClearStrikeMarkers()
	SetMeleeEnabled(akTarget, True)
EndEvent
