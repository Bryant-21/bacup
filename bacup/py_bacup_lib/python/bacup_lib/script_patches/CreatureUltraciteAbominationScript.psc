Function RegisterBossHitEvent()
	If myActor != None && !myActor.IsDead()
		RegisterForHitEvent(myActor)
	EndIf
EndFunction

Function UnregisterBossHitEvent()
	If myActor != None
		UnregisterForHitEvent(myActor)
	EndIf
EndFunction

Function ConfigureBoss(Actor akBoss)
	If akBoss == None || akBoss.IsDead()
		Return
	EndIf

	myActor = akBoss
	If Aggression != None
		myActor.SetValue(Aggression, 2.0)
	EndIf
	If captiveFaction != None
		myActor.RemoveFromFaction(captiveFaction)
	EndIf
	If BlockMutationsKeyword != None
		myActor.RemoveKeyword(BlockMutationsKeyword)
	EndIf
	myActor.EnableAI(True, False)
	RegisterBossHitEvent()
EndFunction

Function SetTitanProtection(Bool abInvulnerable, Bool abEssential)
	ActorBase normalBase = UltraciteTitanForm as ActorBase
	ActorBase finalBase = UltraciteTitanForm_FinalPhase as ActorBase
	If normalBase != None
		normalBase.SetInvulnerable(abInvulnerable)
		normalBase.SetEssential(abEssential)
	EndIf
	If finalBase != None
		finalBase.SetInvulnerable(abInvulnerable)
		finalBase.SetEssential(False)
	EndIf
EndFunction

Function FailTunnelTransition()
	CancelTimer(9002)
	B21TunnelState = 0
	lockMutations = True
	SetTitanProtection(False, False)
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && owningQuest.IsRunning() && !owningQuest.IsStageDone(999)
		owningQuest.SetStage(999)
	EndIf
EndFunction

Bool Function PlayTunnelAnimation(String asEvent, String asCompletion, Float afTimeout)
	If myActor == None
		Return False
	EndIf
	Int polls = 0
	While !myActor.Is3DLoaded() && polls < 20
		Utility.Wait(0.25)
		polls += 1
	EndWhile
	If !myActor.Is3DLoaded() || !RegisterForAnimationEvent(myActor, asCompletion)
		Return False
	EndIf
	If !myActor.PlayAnimation(asEvent)
		UnregisterForAnimationEvent(myActor, asCompletion)
		Return False
	EndIf
	CancelTimer(9002)
	StartTimer(afTimeout, 9002)
	Return True
EndFunction

Actor Function SpawnBossAt(Form akBossForm, ObjectReference akMarker)
	If akBossForm == None || akMarker == None
		Return None
	EndIf

	Actor spawnedBoss = akMarker.PlaceAtMe(akBossForm, 1, True, False, False) as Actor
	If spawnedBoss != None
		ForceRefTo(spawnedBoss)
	EndIf
	Return spawnedBoss
EndFunction

Function ApplyHealthPercent(Actor akBoss, Float afPercent)
	If akBoss == None || HealthAV == None || afPercent >= 1.0
		Return
	EndIf
	Float currentPercent = akBoss.GetValuePercentage(HealthAV)
	If currentPercent <= afPercent || currentPercent <= 0.0
		Return
	EndIf
	Float maxHealth = akBoss.GetValue(HealthAV) / currentPercent
	akBoss.DamageValue(HealthAV, (currentPercent - afPercent) * maxHealth)
EndFunction

Function RestorePhaseFloor(Float afPercent)
	If myActor == None || HealthAV == None || afPercent <= 0.0
		Return
	EndIf
	Float currentPercent = myActor.GetValuePercentage(HealthAV)
	If currentPercent <= 0.0 || currentPercent >= afPercent
		Return
	EndIf
	Float maxHealth = myActor.GetValue(HealthAV) / currentPercent
	myActor.RestoreValue(HealthAV, (afPercent - currentPercent) * maxHealth)
EndFunction

Function BeginBossFight()
	If myActor != None && !myActor.IsDead()
		Return
	EndIf

	currentStageID = 0
	targetStageOnExit = -1
	lockMutations = False
	B21BossInvulnerable = False
	B21TunnelState = 0
	SetTitanProtection(False, True)
	ObjectReference emergeMarker = None
	If MutationStages != None && MutationStages.Length > 0 && MutationStages[0] != None && MutationStages[0].NewPosition != None
		emergeMarker = MutationStages[0].NewPosition.GetRef()
	EndIf
	If emergeMarker == None && Alias_SpawnPoint != None
		emergeMarker = Alias_SpawnPoint.GetRef()
	EndIf

	Actor boss = GetActorRef()
	If boss == None || boss.IsDead()
		boss = SpawnBossAt(UltraciteTitanForm, emergeMarker)
	Else
		If emergeMarker != None
			boss.MoveTo(emergeMarker)
		EndIf
		boss.Enable(True)
	EndIf
	ConfigureBoss(boss)
	If boss != None
		boss.EnableAI(False, False)
		B21TunnelState = 3
		If !PlayTunnelAnimation("TunnelExit", "ReturnToDefault", 9.0)
			FailTunnelTransition()
		EndIf
	EndIf
EndFunction

Function SetBossInvulnerable(Bool abInvulnerable)
	B21BossInvulnerable = abInvulnerable
	If MutationStages != None
		SetTitanProtection(abInvulnerable, currentStageID < MutationStages.Length - 1)
	EndIf
	If abInvulnerable
		B21LockedHealthPercent = 1.0
		If myActor != None && HealthAV != None
			B21LockedHealthPercent = myActor.GetValuePercentage(HealthAV)
		EndIf
		ShowInvulnerableReminder()
	ElseIf VulnerableMessage != None && myActor != None && !myActor.IsDead()
		VulnerableMessage.Show()
	EndIf
	RegisterBossHitEvent()
EndFunction

Function ShowInvulnerableReminder()
	If B21InvulnMessageCooldown || InvulnerableMessage == None
		Return
	EndIf
	InvulnerableMessage.Show()
	If TimeBetweenInvulnMessages > 0
		B21InvulnMessageCooldown = True
		StartTimer(TimeBetweenInvulnMessages as Float, timerID_InvulnerableMSG)
	EndIf
EndFunction

Function HoldLockedHealth()
	If myActor == None || myActor.IsDead() || HealthAV == None
		Return
	EndIf
	; FO76's intact crystal henge blocks all damage; FO4 has no per-source damage gate, so hits are healed back to the phase-start health.
	Float currentPercent = myActor.GetValuePercentage(HealthAV)
	If currentPercent >= B21LockedHealthPercent || currentPercent <= 0.0
		Return
	EndIf
	Float maxHealth = myActor.GetValue(HealthAV) / currentPercent
	myActor.RestoreValue(HealthAV, (B21LockedHealthPercent - currentPercent) * maxHealth)
EndFunction

Function EndBossFight()
	CancelTimer(timerID_StayUnderground)
	CancelTimer(9002)
	targetStageOnExit = -1
	lockMutations = True
	B21BossInvulnerable = False
	B21TunnelState = 0
	SetTitanProtection(False, False)
	UnregisterBossHitEvent()
	Actor boss = myActor
	If boss == None
		boss = GetActorRef()
	EndIf
	If boss != None && !boss.IsDead()
		boss.DisableNoWait(True)
		boss.Delete()
		Clear()
		myActor = None
	EndIf
EndFunction

Function BeginMutation(Int aiNextStage)
	If lockMutations || myActor == None || myActor.IsDead() || MutationStages == None || aiNextStage < 0 || aiNextStage >= MutationStages.Length
		Return
	EndIf

	lockMutations = True
	targetStageOnExit = aiNextStage
	RestorePhaseFloor(MutationStages[aiNextStage].HealthPercent)
	If BlockMutationsKeyword != None
		myActor.AddKeyword(BlockMutationsKeyword)
	EndIf
	If captiveFaction != None
		myActor.AddToFaction(captiveFaction)
	EndIf
	myActor.StopCombat()
	myActor.EnableAI(False, False)
	B21TunnelState = 1
	If !PlayTunnelAnimation("TunnelEnter", "toTunneled", 7.0)
		FailTunnelTransition()
	EndIf
EndFunction

Function FinishRetreat()
	If B21TunnelState != 1 || myActor == None
		Return
	EndIf
	CancelTimer(9002)
	UnregisterForAnimationEvent(myActor, "toTunneled")
	myActor.DisableNoWait(True)
	B21TunnelState = 2
	MutationStage nextStage = MutationStages[targetStageOnExit]
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && nextStage != None && nextStage.TransitionStage >= 0
		owningQuest.SetStage(nextStage.TransitionStage)
	EndIf
	CancelTimer(timerID_StayUnderground)
	If tunnelDuration > 0
		StartTimer(tunnelDuration as Float, timerID_StayUnderground)
	Else
		CompleteMutation()
	EndIf
EndFunction

Function CompleteMutation()
	If B21TunnelState != 2 || targetStageOnExit < 0 || MutationStages == None || targetStageOnExit >= MutationStages.Length
		lockMutations = False
		Return
	EndIf

	MutationStage nextStage = MutationStages[targetStageOnExit]
	ObjectReference destinationMarker
	If nextStage != None && nextStage.NewPosition != None
		destinationMarker = nextStage.NewPosition.GetRef()
	EndIf

	Actor oldBoss = myActor
	Actor nextBoss = oldBoss
	If targetStageOnExit == MutationStages.Length - 1 && UltraciteTitanForm_FinalPhase != None && destinationMarker != None
		Float carriedHealth = 1.0
		If oldBoss != None && HealthAV != None
			carriedHealth = oldBoss.GetValuePercentage(HealthAV)
		EndIf
		Actor finalBoss = SpawnBossAt(UltraciteTitanForm_FinalPhase, destinationMarker)
		If finalBoss != None
			UnregisterBossHitEvent()
			ApplyHealthPercent(finalBoss, carriedHealth)
			nextBoss = finalBoss
			If oldBoss != None && oldBoss != finalBoss
				oldBoss.DisableNoWait()
				oldBoss.Delete()
			EndIf
		EndIf
	EndIf
	If nextBoss != None && nextBoss == oldBoss
		If destinationMarker != None
			nextBoss.MoveTo(destinationMarker)
		EndIf
		nextBoss.Enable(True)
	EndIf
	myActor = nextBoss
	If nextBoss == None
		FailTunnelTransition()
		Return
	EndIf
	nextBoss.EnableAI(False, False)
	B21TunnelState = 3
	If !PlayTunnelAnimation("TunnelExit", "ReturnToDefault", 9.0)
		FailTunnelTransition()
	EndIf
EndFunction

Function FinishEmerge()
	If B21TunnelState != 3 || targetStageOnExit < 0 || MutationStages == None || targetStageOnExit >= MutationStages.Length
		Return
	EndIf
	CancelTimer(9002)
	UnregisterForAnimationEvent(myActor, "ReturnToDefault")
	B21TunnelState = 0
	MutationStage nextStage = MutationStages[targetStageOnExit]
	currentStageID = targetStageOnExit
	targetStageOnExit = -1
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && nextStage != None
		If nextStage.ObjectiveToComplete >= 0 && owningQuest.HasObjective(nextStage.ObjectiveToComplete)
			owningQuest.SetObjectiveCompleted(nextStage.ObjectiveToComplete)
		EndIf
		If nextStage.QuestStageToSet >= 0
			owningQuest.SetStage(nextStage.QuestStageToSet)
		EndIf
	EndIf
	ConfigureBoss(myActor)
	lockMutations = False
EndFunction

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If akSource != myActor
		Return
	EndIf
	If B21TunnelState == 1 && asEventName == "toTunneled"
		FinishRetreat()
	ElseIf B21TunnelState == 3 && asEventName == "ReturnToDefault"
		If targetStageOnExit >= 0
			FinishEmerge()
		Else
			CancelTimer(9002)
			UnregisterForAnimationEvent(myActor, "ReturnToDefault")
			B21TunnelState = 0
			ConfigureBoss(myActor)
		EndIf
	EndIf
EndEvent

Function EvaluateMutation()
	If lockMutations || B21BossInvulnerable || myActor == None || myActor.IsDead() || HealthAV == None || MutationStages == None
		Return
	EndIf
	If MutationEligibleKeyword != None && !myActor.HasKeyword(MutationEligibleKeyword)
		Return
	EndIf

	Int nextStageID = currentStageID + 1
	If nextStageID < MutationStages.Length
		MutationStage nextStage = MutationStages[nextStageID]
		If nextStage != None && myActor.GetValuePercentage(HealthAV) <= nextStage.HealthPercent
			BeginMutation(nextStageID)
		EndIf
	EndIf
EndFunction

Event OnAliasInit()
	currentStageID = 0
	targetStageOnExit = -1
	lockMutations = False
	B21BossInvulnerable = False
	B21TunnelState = 0
	RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
	B21InvulnMessageCooldown = False
	B21LockedHealthPercent = 1.0
	; The quest spawns the Titan when the coaster crystals are cleared (BossBeginStage), not at quest start.
	myActor = GetActorRef()
	ConfigureBoss(myActor)
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender != Game.GetPlayer()
		Return
	EndIf
	If MutationStages != None
		SetTitanProtection(B21BossInvulnerable, currentStageID < MutationStages.Length - 1)
	EndIf
	If B21TunnelState == 1 && !PlayTunnelAnimation("TunnelEnter", "toTunneled", 7.0)
		FailTunnelTransition()
	ElseIf B21TunnelState == 2
		CancelTimer(timerID_StayUnderground)
		StartTimer(tunnelDuration as Float, timerID_StayUnderground)
	ElseIf B21TunnelState == 3 && !PlayTunnelAnimation("TunnelExit", "ReturnToDefault", 9.0)
		FailTunnelTransition()
	EndIf
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	If akTarget == myActor && myActor != None
		If B21BossInvulnerable
			HoldLockedHealth()
			If akAggressor == Game.GetPlayer()
				ShowInvulnerableReminder()
			EndIf
		Else
			EvaluateMutation()
		EndIf
	EndIf
	RegisterBossHitEvent()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 9002 && B21TunnelState != 0
		FailTunnelTransition()
	ElseIf aiTimerID == timerID_StayUnderground
		CompleteMutation()
	ElseIf aiTimerID == timerID_InvulnerableMSG
		B21InvulnMessageCooldown = False
	ElseIf aiTimerID == timerID_DeleteTitan && myActor != None && myActor.IsDead()
		myActor.DisableNoWait()
		myActor.Delete()
		Clear()
		myActor = None
	EndIf
EndEvent

Event OnDeath(Actor akKiller)
	CancelTimer(timerID_StayUnderground)
	CancelTimer(9002)
	lockMutations = True
	B21BossInvulnerable = False
	B21TunnelState = 0
	SetTitanProtection(False, False)
	If DeleteTitanTimerDuration > 0
		StartTimer(DeleteTitanTimerDuration as Float, timerID_DeleteTitan)
	EndIf
EndEvent

Event OnAliasShutdown()
	CancelTimer(timerID_StayUnderground)
	CancelTimer(9002)
	CancelTimer(timerID_DeleteTitan)
	CancelTimer(timerID_InvulnerableMSG)
	UnregisterBossHitEvent()
	If myActor != None
		UnregisterForAnimationEvent(myActor, "toTunneled")
		UnregisterForAnimationEvent(myActor, "ReturnToDefault")
	EndIf
	UnregisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
	B21BossInvulnerable = False
	B21TunnelState = 0
	SetTitanProtection(False, False)
	myActor = None
EndEvent
