Event OnQuestInit()
	progressPercentage = 0.0
	SetObjectiveDisplayed(DefeatAttackersObjectiveIndex, False)
	SetObjectiveCompleted(DefeatAttackersObjectiveIndex, False)
	SetObjectiveFailed(DefeatAttackersObjectiveIndex, False)
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	If QuestInitCompleteStage >= 0 && !IsStageDone(QuestInitCompleteStage)
		SetStage(QuestInitCompleteStage)
	EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender != Game.GetPlayer() || !IsRunning() || IsStageDone(DefeatAttackersStage)
		Return
	EndIf
	BlockWorkshop(True)
	StartTimer(3.0, 301)
EndEvent

Event OnQuestShutdown()
	CancelTimer(301)
	CancelTimer(302)
	CancelTimer(303)
	BlockWorkshop(False)
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer() as ObjectReference || !IsRunning()
		Return
	EndIf
	If IsStageDone(DefeatAttackersStage) || LivingAttackerCount() <= 0
		Return
	EndIf
	If GQ_WorkshopTakeoverActivationBlockedMessage != None
		GQ_WorkshopTakeoverActivationBlockedMessage.Show()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 301
		If !IsRunning() || IsStageDone(DefeatAttackersStage)
			Return
		EndIf
		If !HasAnyAttacker()
			If RecruitAttackers() > 0
				StartAttackerWave()
			EndIf
		Else
			UpdateProgress()
			If LivingAttackerCount() <= 0
				SetStage(DefeatAttackersStage)
				Return
			EndIf
		EndIf
		StartTimer(3.0, 301)
	ElseIf aiTimerID == 302
		If IsRunning()
			Stop()
		EndIf
	ElseIf aiTimerID == 303
		; FO76 handed the surviving Defend attackers to this quest. With none of
		; them left there is nothing to retake, so shut down instead of showing a
		; permanent objective with no target.
		If IsRunning() && !HasAnyAttacker() && !IsStageDone(DefeatAttackersStage)
			Stop()
		EndIf
	EndIf
EndEvent

DefaultQuestEncounterWaveScript Function EncounterWaveScript()
	Quest owner = Self as Quest
	Return owner as DefaultQuestEncounterWaveScript
EndFunction

ObjectReference Function WorkshopReference()
	If Alias_Workshop == None
		Return None
	EndIf
	Return Alias_Workshop.GetReference()
EndFunction

ObjectReference Function AttackOrigin()
	Quest owner = Self as Quest
	ReferenceAlias centerMarker = owner.GetAlias(4) as ReferenceAlias
	If centerMarker != None && centerMarker.GetReference() != None
		Return centerMarker.GetReference()
	EndIf
	Return WorkshopReference()
EndFunction

Function BeginTakeover()
	OriginalWorkshopState = CurrentWorkshopOwnership()
	BlockWorkshop(True)
	SetObjectiveDisplayed(DefeatAttackersObjectiveIndex, True, True)
	If RecruitAttackers() > 0
		StartAttackerWave()
	EndIf
	StartTimer(3.0, 301)
	CancelTimer(303)
	StartTimer(45.0, 303)
EndFunction

Function StartAttackerWave()
	NameAttackers()
	DefaultQuestEncounterWaveScript waveScript = EncounterWaveScript()
	If waveScript != None
		waveScript.StartEncounterWave(0)
	EndIf
	If AttackersInitCompleteStage >= 0 && !IsStageDone(AttackersInitCompleteStage)
		SetStage(AttackersInitCompleteStage)
	EndIf
EndFunction

Int Function RecruitAttackers()
	; The wave row carries no EMS wave, only IncludeOtherNearbyActors plus the
	; keyword list the Defend event's attacker aliases stamp on their actors.
	If Alias_Attackers == None
		Return 0
	EndIf
	ObjectReference origin = AttackOrigin()
	DefaultQuestEncounterWaveScript waveScript = EncounterWaveScript()
	If origin == None || waveScript == None || waveScript.EncounterWaves == None || waveScript.EncounterWaves.Length <= 0
		Return 0
	EndIf
	FormList filterKeywords = waveScript.EncounterWaves[0].FilterKeywords
	If filterKeywords == None
		Return 0
	EndIf
	Float radius = waveScript.EncounterWaves[0].SpawnAreaRadiusMax
	If radius <= 0.0
		radius = 5120.0
	EndIf

	Int recruited = 0
	ObjectReference[] nearby = origin.FindAllReferencesWithKeyword(filterKeywords, radius)
	Int index = 0
	While nearby != None && index < nearby.Length
		Actor candidate = nearby[index] as Actor
		If candidate != None && !candidate.IsDead() && candidate != Game.GetPlayer() && Alias_Attackers.Find(candidate) < 0
			Alias_Attackers.AddRef(candidate)
			recruited += 1
		EndIf
		index += 1
	EndWhile
	Return recruited
EndFunction

Bool Function HasAnyAttacker()
	Return Alias_Attackers != None && Alias_Attackers.GetCount() > 0
EndFunction

Int Function LivingAttackerCount()
	If Alias_Attackers == None
		Return 0
	EndIf
	Int living = 0
	Int index = 0
	While index < Alias_Attackers.GetCount()
		Actor attacker = Alias_Attackers.GetAt(index) as Actor
		If attacker != None && !attacker.IsDead()
			living += 1
		EndIf
		index += 1
	EndWhile
	Return living
EndFunction

Function UpdateProgress()
	If Alias_Attackers == None || Alias_Attackers.GetCount() <= 0
		Return
	EndIf
	Int total = Alias_Attackers.GetCount()
	progressPercentage = (total - LivingAttackerCount()) as Float / total as Float
EndFunction

Function NameAttackers()
	If Alias_AttackerName == None || Alias_AttackerName.GetReference() != None || Alias_Attackers == None
		Return
	EndIf
	Int index = 0
	While index < Alias_Attackers.GetCount()
		Actor attacker = Alias_Attackers.GetAt(index) as Actor
		If attacker != None && !attacker.IsDead()
			Alias_AttackerName.ForceRefTo(attacker)
			Return
		EndIf
		index += 1
	EndWhile
EndFunction

Int Function CurrentWorkshopOwnership()
	WorkshopScript workshopRef = WorkshopReference() as WorkshopScript
	If workshopRef == None
		Return -1
	EndIf
	If workshopRef.OwnedByPlayer
		Return 1
	EndIf
	Return 0
EndFunction

Function BlockWorkshop(Bool abBlocked)
	ObjectReference workshopRef = WorkshopReference()
	If workshopRef == None
		Return
	EndIf
	If abBlocked
		RegisterForRemoteEvent(workshopRef, "OnActivate")
		workshopRef.BlockActivation(True)
		Return
	EndIf
	UnregisterForRemoteEvent(workshopRef, "OnActivate")
	; Fallout 4's own WorkshopScript keeps activation blocked while unowned.
	workshopRef.BlockActivation(CurrentWorkshopOwnership() == 0)
EndFunction

Function RestoreWorkshopOwnership()
	If OriginalWorkshopState != 1
		Return
	EndIf
	WorkshopScript workshopRef = WorkshopReference() as WorkshopScript
	If workshopRef != None && !workshopRef.OwnedByPlayer
		workshopRef.SetOwnedByPlayer(True)
	EndIf
EndFunction

Function CompleteTakeover()
	CancelTimer(301)
	CancelTimer(303)
	progressPercentage = 1.0
	If IsObjectiveDisplayed(DefeatAttackersObjectiveIndex) && !IsObjectiveCompleted(DefeatAttackersObjectiveIndex)
		SetObjectiveCompleted(DefeatAttackersObjectiveIndex, True)
	EndIf
	RestoreWorkshopOwnership()
	CancelTimer(302)
	StartTimer(10.0, 302)
EndFunction

Function CleanupTakeover()
	CancelTimer(301)
	CancelTimer(302)
	CancelTimer(303)
	If IsObjectiveDisplayed(DefeatAttackersObjectiveIndex) && !IsObjectiveCompleted(DefeatAttackersObjectiveIndex)
		SetObjectiveFailed(DefeatAttackersObjectiveIndex, True)
	EndIf
	BlockWorkshop(False)
	DefaultQuestEncounterWaveScript waveScript = EncounterWaveScript()
	If waveScript != None
		waveScript.StopAllEncounterWaves(False)
	EndIf
EndFunction
