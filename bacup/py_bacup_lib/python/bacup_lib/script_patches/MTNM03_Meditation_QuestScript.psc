ReferenceAlias Function HubAlias(Int aiHub)
	If aiHub == 0
		Return MTNM03_Hub01
	ElseIf aiHub == 1
		Return MTNM03_Hub02
	ElseIf aiHub == 2
		Return MTNM03_Hub03
	ElseIf aiHub == 3
		Return MTNM03_Hub04
	EndIf
	Return None
EndFunction

ObjectReference Function HubRef(Int aiHub)
	ReferenceAlias hubAlias = HubAlias(aiHub)
	If hubAlias == None
		Return None
	EndIf
	Return hubAlias.GetReference()
EndFunction

Int Function FindHub(ObjectReference akRef)
	If akRef == None
		Return -1
	EndIf
	Int hub = 0
	While hub < 4
		If HubRef(hub) == akRef
			Return hub
		EndIf
		hub += 1
	EndWhile
	Return -1
EndFunction

Int Function CountIntactHubs()
	Int intact = 0
	Int hub = 0
	While hub < 4
		ObjectReference hubRef = HubRef(hub)
		If hubRef != None && !hubRef.IsDestroyed()
			intact += 1
		EndIf
		hub += 1
	EndWhile
	Return intact
EndFunction

Spell Function ZenSpellForRank(Int aiRank)
	If aiRank == 1
		Return MTNM03_ZenSpell01
	ElseIf aiRank == 2
		Return MTNM03_ZenSpell02
	ElseIf aiRank == 3
		Return MTNM03_ZenSpell03
	ElseIf aiRank == 4
		Return MTNM03_ZenSpell04
	EndIf
	Return None
EndFunction

Spell Function ZenRewardForRank(Int aiRank)
	If aiRank == 1
		Return MTNM03_ZenSpell_Reward_01
	ElseIf aiRank == 2
		Return MTNM03_ZenSpell_Reward_02
	ElseIf aiRank == 3
		Return MTNM03_ZenSpell_Reward_03
	ElseIf aiRank == 4
		Return MTNM03_ZenSpell_Reward_04
	EndIf
	Return None
EndFunction

Bool Function IsPlayerParticipating()
	Quest owner = Self as Quest
	DefaultEventQuest eventQuest = owner as DefaultEventQuest
	Return eventQuest == None || eventQuest.IsPlayerParticipating()
EndFunction

Function DispelZenBuffs()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf
	Int rank = 1
	While rank <= 4
		Spell zenSpell = ZenSpellForRank(rank)
		If zenSpell != None
			playerRef.DispelSpell(zenSpell)
		EndIf
		rank += 1
	EndWhile
EndFunction

; The in-event Zen Mastery rank equals the number of hubs still playing.
Function ApplyZenRank(Int aiRank)
	If !IsPlayerParticipating()
		aiRank = 0
	EndIf
	If aiRank == TotalRemaining
		Return
	EndIf
	TotalRemaining = aiRank
	DispelZenBuffs()
	Actor playerRef = Game.GetPlayer()
	Spell zenSpell = ZenSpellForRank(aiRank)
	If playerRef != None && zenSpell != None
		zenSpell.Cast(playerRef, playerRef)
	EndIf
EndFunction

Function SetEnabled(ReferenceAlias akAlias, Bool abEnabled)
	If akAlias == None || akAlias.GetReference() == None
		Return
	EndIf
	If abEnabled
		akAlias.GetReference().Enable(False)
	Else
		akAlias.GetReference().Disable(False)
	EndIf
EndFunction

Bool Function UpdateHubObjectives(Int aiHub, Bool abDown)
	Int defendObjective = 30 + aiHub * 10
	Int repairObjective = 70 + aiHub * 10
	If abDown
		If IsObjectiveDisplayed(repairObjective)
			Return False
		EndIf
		SetObjectiveDisplayed(defendObjective, False)
		SetObjectiveDisplayed(repairObjective, True, True)
		Return True
	ElseIf IsObjectiveDisplayed(defendObjective)
		Return False
	EndIf
	SetObjectiveDisplayed(repairObjective, False)
	SetObjectiveDisplayed(defendObjective, True, True)
	Return True
EndFunction

Function ArmHubRepair(ObjectReference akHub, Bool abDown)
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || akHub == None
		Return
	EndIf
	UnregisterForDistanceEvents(playerRef, akHub)
	If abDown && AreaSetupDone == 1
		RegisterForDistanceLessThanEvent(playerRef, akHub, 250.0)
	EndIf
EndFunction

Function ReconcileHubs()
	If AreaSetupDone != 1 || CleanupDone == 1 || !IsRunning()
		Return
	EndIf
	Int hub = 0
	While hub < 4
		ObjectReference hubRef = HubRef(hub)
		If hubRef != None
			Bool down = hubRef.IsDestroyed()
			; Re-arming only on a transition keeps a player without steel from being re-prompted every reconcile.
			If UpdateHubObjectives(hub, down)
				ArmHubRepair(hubRef, down)
			EndIf
		EndIf
		hub += 1
	EndWhile
	ApplyZenRank(CountIntactHubs())
EndFunction

Function ResetMeditation()
	CancelTimer(12671)
	CancelTimer(12672)
	UnregisterForAllEvents()
	AreaSetupDone = 0
	CleanupDone = 0
	InitialSetupDone = 1
	TotalRemaining = 0
	DispelZenBuffs()
	SetEnabled(MNTM03_BrazierFlames, False)

	; FO76 players light the brazier through an activation box; standing on it also counts in case the box is not pickable.
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && MNTM03_BrazierStartTrigger != None && MNTM03_BrazierStartTrigger.GetReference() != None
		RegisterForDistanceLessThanEvent(playerRef, MNTM03_BrazierStartTrigger.GetReference(), 128.0)
	EndIf
EndFunction

Function StartMeditation()
	If AreaSetupDone == 1 || CleanupDone == 1
		Return
	EndIf
	AreaSetupDone = 1
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && MNTM03_BrazierStartTrigger != None && MNTM03_BrazierStartTrigger.GetReference() != None
		UnregisterForDistanceEvents(playerRef, MNTM03_BrazierStartTrigger.GetReference())
	EndIf
	SetEnabled(MNTM03_BrazierFlames, True)
	Int hub = 0
	While hub < 4
		ObjectReference hubRef = HubRef(hub)
		If hubRef != None
			RegisterForRemoteEvent(hubRef, "OnDestructionStageChanged")
		EndIf
		hub += 1
	EndWhile
	ReconcileHubs()
	StartTimer(5.0, 12671)
EndFunction

Function EndMeditation(Bool abSuccess, Message akRewardMessage)
	If CleanupDone == 1
		Return
	EndIf
	CleanupDone = 1
	AreaSetupDone = 0
	CancelTimer(12671)
	Actor playerRef = Game.GetPlayer()
	Int hub = 0
	While hub < 4
		ObjectReference hubRef = HubRef(hub)
		If hubRef != None
			UnregisterForRemoteEvent(hubRef, "OnDestructionStageChanged")
			If playerRef != None
				UnregisterForDistanceEvents(playerRef, hubRef)
			EndIf
			Int defendObjective = 30 + hub * 10
			Int repairObjective = 70 + hub * 10
			If abSuccess && !hubRef.IsDestroyed()
				SetObjectiveCompleted(defendObjective, True)
				SetObjectiveDisplayed(repairObjective, False)
			Else
				If IsObjectiveDisplayed(defendObjective) && !IsObjectiveCompleted(defendObjective)
					SetObjectiveFailed(defendObjective, True)
				EndIf
				If IsObjectiveDisplayed(repairObjective) && !IsObjectiveCompleted(repairObjective)
					SetObjectiveFailed(repairObjective, True)
				EndIf
			EndIf
		EndIf
		hub += 1
	EndWhile

	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
	If waves != None
		waves.StopAllEncounterWaves(False)
	EndIf

	Int rank = 0
	If abSuccess && IsPlayerParticipating()
		rank = CountIntactHubs()
	EndIf
	DispelZenBuffs()
	TotalRemaining = 0
	Spell reward = ZenRewardForRank(rank)
	If playerRef != None && reward != None
		reward.Cast(playerRef, playerRef)
		If akRewardMessage != None
			akRewardMessage.Show()
		EndIf
	EndIf
	StartTimer(15.0, 12672)
EndFunction

Event ObjectReference.OnDestructionStageChanged(ObjectReference akSender, Int aiOldStage, Int aiCurrentStage)
	ReconcileHubs()
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
	Actor playerRef = Game.GetPlayer()
	ObjectReference target = akObj2
	If target == playerRef
		target = akObj1
	EndIf
	If target == None || playerRef == None
		Return
	EndIf

	If MNTM03_BrazierStartTrigger != None && target == MNTM03_BrazierStartTrigger.GetReference()
		UnregisterForDistanceEvents(playerRef, target)
		If IsRunning() && !IsStageDone(30) && !IsStageDone(9991) && !IsStageDone(300)
			SetStage(30)
		EndIf
		Return
	EndIf

	If FindHub(target) < 0 || AreaSetupDone != 1 || !target.IsDestroyed()
		Return
	EndIf
	UnregisterForDistanceEvents(playerRef, target)
	If !DefaultAliasOnObjectRepaired.TryRepairObject(target, playerRef)
		RegisterForDistanceGreaterThanEvent(playerRef, target, 600.0)
	EndIf
EndEvent

Event OnDistanceGreaterThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
	ObjectReference target = akObj2
	If target == Game.GetPlayer()
		target = akObj1
	EndIf
	If FindHub(target) >= 0
		ArmHubRepair(target, target.IsDestroyed())
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 12671
		If AreaSetupDone == 1 && CleanupDone != 1 && IsRunning()
			ReconcileHubs()
			StartTimer(5.0, 12671)
		EndIf
	ElseIf aiTimerID == 12672
		If IsRunning()
			Stop()
		EndIf
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(12671)
	CancelTimer(12672)
	UnregisterForAllEvents()
	DispelZenBuffs()
	SetEnabled(MNTM03_BrazierFlames, False)
	AreaSetupDone = 0
	CleanupDone = 1
	TotalRemaining = 0
EndEvent
