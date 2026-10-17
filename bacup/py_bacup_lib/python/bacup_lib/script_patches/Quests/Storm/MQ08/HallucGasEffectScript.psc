; FO4 has no OnEffectFinishShared, and a non-native script cannot declare a new event.
; @drop-member OnEffectFinishShared
; The FO76 server spawned the hallucinated shadows (Storm_MQ08_LvlHallucGasShadow): every fMinSpawnTime-fMaxSpawnTime
; seconds a shadow appears near the gassed player and attacks, up to iMaxShadows alive, until OwningBaseQuest reaches a
; ShadowBlockSpawnStages stage. When the gas wears off the remaining shadows dissolve (their ShadowEffect smokes them).
Event OnEffectStart(Actor akTarget, Actor akCaster)
	myQuestInstance = OwningBaseQuest
	spawnedShadows = new Actor[0]
	StartTimer(GetSoundTimer(), iSoundTimerID)
	StartTimer(GetShadowTimer(), iSpawnShadowTimerID)
	If ImageSpaceToApply
		ImageSpaceToApply.Apply(1.0)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == iSoundTimerID
		PlayRandomSound()
		StartTimer(GetSoundTimer(), iSoundTimerID)
	ElseIf aiTimerID == iSpawnShadowTimerID
		If !ShadowSpawnBlocked()
			SpawnShadow()
			StartTimer(GetShadowTimer(), iSpawnShadowTimerID)
		EndIf
	EndIf
EndEvent

Bool Function ShadowSpawnBlocked()
	If myQuestInstance == None || ShadowBlockSpawnStages == None
		Return False
	EndIf
	Int index = 0
	While index < ShadowBlockSpawnStages.Length
		If myQuestInstance.IsStageDone(ShadowBlockSpawnStages[index])
			Return True
		EndIf
		index += 1
	EndWhile
	Return False
EndFunction

Function SpawnShadow()
	Actor target = GetTargetActor()
	If target == None || ShadowEnemy == None || target.IsDead()
		Return
	EndIf
	If spawnedShadows == None
		spawnedShadows = new Actor[0]
	EndIf
	Int index = spawnedShadows.Length - 1
	While index >= 0
		If spawnedShadows[index] == None || spawnedShadows[index].IsDead()
			spawnedShadows.Remove(index)
		EndIf
		index -= 1
	EndWhile
	If spawnedShadows.Length >= iMaxShadows
		Return
	EndIf
	Actor shadow = target.PlaceActorAtMe(ShadowEnemy)
	If shadow == None
		Return
	EndIf
	Float angle = Utility.RandomFloat(0.0, 360.0)
	Float distance = Utility.RandomFloat(512.0, 1024.0)
	shadow.SetPosition(target.GetPositionX() + distance * Math.Sin(angle), target.GetPositionY() + distance * Math.Cos(angle), target.GetPositionZ())
	shadow.MoveToNearestNavmeshLocation()
	spawnedShadows.Add(shadow)
	RegisterForRemoteEvent(shadow, "OnDying")
	shadow.StartCombat(target)
EndFunction

Event Actor.OnDying(Actor akSender, Actor akKiller)
	UnregisterForRemoteEvent(akSender, "OnDying")
	If spawnedShadows != None
		Int index = spawnedShadows.Find(akSender)
		If index >= 0
			spawnedShadows.Remove(index)
		EndIf
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	CancelTimer(iSoundTimerID)
	CancelTimer(iSpawnShadowTimerID)
	UnregisterForAllRemoteEvents()
	If ImageSpaceToApply
		ImageSpaceToApply.Remove()
	EndIf
	If spawnedShadows != None
		Int index = 0
		While index < spawnedShadows.Length
			If spawnedShadows[index] != None && !spawnedShadows[index].IsDead()
				spawnedShadows[index].Kill()
			EndIf
			index += 1
		EndWhile
		spawnedShadows = None
	EndIf
EndEvent
