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

Function BeginMutation(Int aiNextStage)
	If lockMutations || myActor == None || myActor.IsDead() || MutationStages == None || aiNextStage < 0 || aiNextStage >= MutationStages.Length
		Return
	EndIf

	; Encounter-wave IDs are consumed by a hollow server quest controller, so phase changes stay actor-local.
	lockMutations = True
	targetStageOnExit = aiNextStage
	MutationStage nextStage = MutationStages[aiNextStage]
	If BlockMutationsKeyword != None
		myActor.AddKeyword(BlockMutationsKeyword)
	EndIf
	If captiveFaction != None
		myActor.AddToFaction(captiveFaction)
	EndIf
	myActor.EnableAI(False, False)

	If CustomMutationHealSpell != None
		CustomMutationHealSpell.Cast(myActor, myActor)
	ElseIf DefaultMutationHealSpell != None
		DefaultMutationHealSpell.Cast(myActor, myActor)
	EndIf
	If InvulnerableMessage != None
		InvulnerableMessage.Show()
	EndIf

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
	If targetStageOnExit < 0 || MutationStages == None || targetStageOnExit >= MutationStages.Length
		lockMutations = False
		Return
	EndIf

	MutationStage nextStage = MutationStages[targetStageOnExit]
	ObjectReference destinationMarker
	If nextStage != None && nextStage.NewPosition != None
		destinationMarker = nextStage.NewPosition.GetRef()
	EndIf

	Form nextBossForm = UltraciteTitanForm
	If targetStageOnExit == MutationStages.Length - 1 && UltraciteTitanForm_FinalPhase != None
		nextBossForm = UltraciteTitanForm_FinalPhase
	EndIf

	Actor oldBoss = myActor
	Actor nextBoss = SpawnBossAt(nextBossForm, destinationMarker)
	If nextBoss != None
		UnregisterBossHitEvent()
		myActor = nextBoss
		ConfigureBoss(nextBoss)
		If oldBoss != None && oldBoss != nextBoss
			oldBoss.DisableNoWait()
			oldBoss.Delete()
		EndIf
	ElseIf oldBoss != None && destinationMarker != None
		oldBoss.MoveTo(destinationMarker)
		ConfigureBoss(oldBoss)
	EndIf

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
	If VulnerableMessage != None
		VulnerableMessage.Show()
	EndIf
	lockMutations = False
EndFunction

Function EvaluateMutation()
	If lockMutations || myActor == None || myActor.IsDead() || HealthAV == None || MutationStages == None
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
	myActor = GetActorRef()
	If myActor == None && Alias_SpawnPoint != None
		myActor = SpawnBossAt(UltraciteTitanForm, Alias_SpawnPoint.GetRef())
	EndIf
	ConfigureBoss(myActor)
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	If akTarget == myActor
		EvaluateMutation()
	EndIf
	RegisterBossHitEvent()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == timerID_StayUnderground
		CompleteMutation()
	ElseIf aiTimerID == timerID_DeleteTitan && myActor != None && myActor.IsDead()
		myActor.DisableNoWait()
		myActor.Delete()
		Clear()
		myActor = None
	EndIf
EndEvent

Event OnDeath(Actor akKiller)
	CancelTimer(timerID_StayUnderground)
	lockMutations = True
	If DeleteTitanTimerDuration > 0
		StartTimer(DeleteTitanTimerDuration as Float, timerID_DeleteTitan)
	EndIf
EndEvent

Event OnAliasShutdown()
	CancelTimer(timerID_StayUnderground)
	CancelTimer(timerID_DeleteTitan)
	UnregisterBossHitEvent()
	myActor = None
EndEvent
