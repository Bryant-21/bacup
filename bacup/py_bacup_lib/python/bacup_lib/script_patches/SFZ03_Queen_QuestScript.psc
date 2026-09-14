Event OnQuestInit()
	RegisterRuntimeEvents()
EndEvent

Event OnQuestShutdown()
	UnregisterRuntimeEvents()
	ResetHuntState()
EndEvent

Function RegisterRuntimeEvents()
	ReferenceAlias siteOneAlias = GetAlias(50) as ReferenceAlias
	ReferenceAlias siteTwoAlias = GetAlias(51) as ReferenceAlias
	ReferenceAlias siteThreeAlias = GetAlias(52) as ReferenceAlias
	ReferenceAlias analyzerAlias = GetAlias(10) as ReferenceAlias

	ObjectReference siteOneTrigger = None
	ObjectReference siteTwoTrigger = None
	ObjectReference siteThreeTrigger = None
	ObjectReference analyzerRef = None
	If siteOneAlias != None
		siteOneTrigger = siteOneAlias.GetReference()
	EndIf
	If siteTwoAlias != None
		siteTwoTrigger = siteTwoAlias.GetReference()
	EndIf
	If siteThreeAlias != None
		siteThreeTrigger = siteThreeAlias.GetReference()
	EndIf
	If analyzerAlias != None
		analyzerRef = analyzerAlias.GetReference()
	EndIf

	If siteOneTrigger != None
		UnregisterForRemoteEvent(siteOneTrigger, "OnTriggerEnter")
		RegisterForRemoteEvent(siteOneTrigger, "OnTriggerEnter")
	EndIf
	If siteTwoTrigger != None
		UnregisterForRemoteEvent(siteTwoTrigger, "OnTriggerEnter")
		RegisterForRemoteEvent(siteTwoTrigger, "OnTriggerEnter")
	EndIf
	If siteThreeTrigger != None
		UnregisterForRemoteEvent(siteThreeTrigger, "OnTriggerEnter")
		RegisterForRemoteEvent(siteThreeTrigger, "OnTriggerEnter")
	EndIf
	If analyzerRef != None
		UnregisterForRemoteEvent(analyzerRef, "OnActivate")
		RegisterForRemoteEvent(analyzerRef, "OnActivate")
	EndIf
EndFunction

Function UnregisterRuntimeEvents()
	ReferenceAlias siteOneAlias = GetAlias(50) as ReferenceAlias
	ReferenceAlias siteTwoAlias = GetAlias(51) as ReferenceAlias
	ReferenceAlias siteThreeAlias = GetAlias(52) as ReferenceAlias
	ReferenceAlias analyzerAlias = GetAlias(10) as ReferenceAlias

	ObjectReference siteOneTrigger = None
	ObjectReference siteTwoTrigger = None
	ObjectReference siteThreeTrigger = None
	ObjectReference analyzerRef = None
	If siteOneAlias != None
		siteOneTrigger = siteOneAlias.GetReference()
	EndIf
	If siteTwoAlias != None
		siteTwoTrigger = siteTwoAlias.GetReference()
	EndIf
	If siteThreeAlias != None
		siteThreeTrigger = siteThreeAlias.GetReference()
	EndIf
	If analyzerAlias != None
		analyzerRef = analyzerAlias.GetReference()
	EndIf

	If siteOneTrigger != None
		UnregisterForRemoteEvent(siteOneTrigger, "OnTriggerEnter")
	EndIf
	If siteTwoTrigger != None
		UnregisterForRemoteEvent(siteTwoTrigger, "OnTriggerEnter")
	EndIf
	If siteThreeTrigger != None
		UnregisterForRemoteEvent(siteThreeTrigger, "OnTriggerEnter")
	EndIf
	If analyzerRef != None
		UnregisterForRemoteEvent(analyzerRef, "OnActivate")
	EndIf
EndFunction

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer() || !IsStageDone(100) || IsStageDone(180)
		Return
	EndIf

	ReferenceAlias siteOneAlias = GetAlias(50) as ReferenceAlias
	ReferenceAlias siteTwoAlias = GetAlias(51) as ReferenceAlias
	ReferenceAlias siteThreeAlias = GetAlias(52) as ReferenceAlias
	If siteOneAlias != None && akSender == siteOneAlias.GetReference()
		SetStage(110)
	ElseIf siteTwoAlias != None && akSender == siteTwoAlias.GetReference()
		SetStage(120)
	ElseIf siteThreeAlias != None && akSender == siteThreeAlias.GetReference()
		SetStage(130)
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	ReferenceAlias analyzerAlias = GetAlias(10) as ReferenceAlias
	If akActionRef != playerRef || analyzerAlias == None || akSender != analyzerAlias.GetReference() || !IsStageDone(300) || IsStageDone(400)
		Return
	EndIf

	ObjectReference sampleRef = CryptidSample.GetReference()
	If sampleRef == None
		Return
	EndIf

	Form sampleForm = sampleRef.GetBaseObject()
	If sampleForm != None && playerRef.GetItemCount(sampleForm) > 0
		playerRef.RemoveItem(sampleForm, 1, True)
		SetStage(400)
	EndIf
EndEvent

Function ResetHuntState()
	BossLocation = 0
	CreatureStage = -1
EndFunction

Bool Function InspectCryptidCollection(RefCollectionAlias akCollection, Int aiLocation)
	If akCollection == None
		Return False
	EndIf

	Int index = 0
	While index < akCollection.GetCount()
		ObjectReference candidate = akCollection.GetAt(index)
		If candidate != None && candidate.HasKeyword(SFZ03_Queen_CryptidBossKeyword)
			BossLocation = aiLocation
			HandleCryptidEncounter(candidate, akCollection)
			Return True
		EndIf
		index += 1
	EndWhile
	Return False
EndFunction

Function HandleCryptidEncounter(ObjectReference akCryptid, RefCollectionAlias akCollection)
	If akCryptid == None || !akCryptid.HasKeyword(SFZ03_Queen_CryptidBossKeyword)
		Return
	EndIf

	RefCollectionAlias bossOne = GetAlias(42) as RefCollectionAlias
	RefCollectionAlias bossTwo = GetAlias(45) as RefCollectionAlias
	RefCollectionAlias bossThree = GetAlias(46) as RefCollectionAlias
	If akCollection == bossOne
		BossLocation = 1
	ElseIf akCollection == bossTwo
		BossLocation = 2
	ElseIf akCollection == bossThree
		BossLocation = 3
	EndIf

	If !IsStageDone(180)
		SetStage(180)
	EndIf
	SetCreatureStageFromActor(akCryptid as Actor)
EndFunction

Function HandleCryptidDeath(ObjectReference akCryptid, RefCollectionAlias akCollection)
	Actor cryptid = akCryptid as Actor
	If cryptid == None || !cryptid.IsDead() || !cryptid.HasKeyword(SFZ03_Queen_CryptidBossKeyword)
		Return
	EndIf

	HandleCryptidEncounter(cryptid, akCollection)
	ObjectReference deathMarkerRef = DeathMarker.GetReference()
	If deathMarkerRef != None
		deathMarkerRef.MoveTo(cryptid)
	EndIf
	If !IsStageDone(200)
		SetStage(200)
	EndIf
EndFunction

Function HarvestCryptidSample(ObjectReference akCryptid, ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	Actor cryptid = akCryptid as Actor
	If akActionRef != playerRef || cryptid == None || !cryptid.IsDead() || !cryptid.HasKeyword(SFZ03_Queen_CryptidBossKeyword) || !IsStageDone(200) || IsStageDone(300)
		Return
	EndIf

	ObjectReference sampleRef = CryptidSample.GetReference()
	If sampleRef == None
		Return
	EndIf

	Form sampleForm = sampleRef.GetBaseObject()
	If sampleForm == None
		Return
	EndIf
	If playerRef.GetItemCount(sampleForm) == 0
		playerRef.AddItem(sampleForm, 1, True)
	EndIf
	SetStage(300)
EndFunction

Function SetCreatureStageFromActor(Actor akCryptid)
	If akCryptid == None
		Return
	EndIf

	ActorBase cryptidBase = akCryptid.GetLeveledActorBase()
	If cryptidBase == None
		Return
	EndIf

	Race cryptidRace = cryptidBase.GetRace()
	Int index = 0
	While index < StagesAndSpells.Length
		StageAndSpell mapping = StagesAndSpells[index]
		If mapping.VictimRace == cryptidRace
			SetCreatureStage(mapping.QuestStage)
			If !IsStageDone(mapping.QuestStage)
				SetStage(mapping.QuestStage)
			EndIf
			Return
		EndIf
		index += 1
	EndWhile
EndFunction

Function SetCreatureStage(Int aiStage)
	CreatureStage = aiStage
EndFunction

Function CastCryptidKnowledge()
	Actor playerRef = Game.GetPlayer()
	Int index = 0
	While index < StagesAndSpells.Length
		StageAndSpell mapping = StagesAndSpells[index]
		If mapping.QuestStage == CreatureStage && mapping.SpellToUse != None
			mapping.SpellToUse.Cast(playerRef, playerRef)
			Return
		EndIf
		index += 1
	EndWhile
EndFunction
