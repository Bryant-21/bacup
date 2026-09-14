; BeeSwarm.nif has no looping "idle" sequence: its behavior graph opens on SwarmFXStart,
; a 0.47s CYCLE_CLAMP one-shot, and only the beeSwarmStage* anim events reach the looping
; SwarmFXA/B/C states. FO76 stripped this script server-side, leaving every
; PlaySubGraphAnimation inside an OnBeginState -- and an auto state's OnBeginState does not
; run at initialization -- so no event was ever sent and the swarm rendered nothing.

Function StartSwarmStage(String asEventName)
	If selfActorRef != None && asEventName != ""
		selfActorRef.PlaySubGraphAnimation(asEventName)
	EndIf
EndFunction

Function ArmHitWatch()
	; RegisterForHitEvent delivers a single OnHit, so it is re-armed after every hit.
	If selfActorRef != None && !selfActorRef.IsDead()
		RegisterForHitEvent(selfActorRef)
	EndIf
EndFunction

Function EvaluateHealthStage()
	If selfActorRef == None || Health == None || selfActorRef.IsDead()
		Return
	EndIf

	String currentState = GetState()
	If currentState == "disperse" || currentState == "death"
		Return
	EndIf

	Float healthPercent = selfActorRef.GetValuePercentage(Health)
	If healthPercent <= HealthPercentLow
		If currentState != "healthlow"
			GoToState("healthlow")
		EndIf
	ElseIf healthPercent <= HealthPercentMid
		If currentState != "healthmid"
			GoToState("healthmid")
		EndIf
	EndIf
EndFunction

Event OnEffectStart(actor akTarget, actor akCaster)
	selfActorRef = akCaster
	If selfActorRef == None
		selfActorRef = akTarget
	EndIf
	If selfActorRef == None
		Return
	EndIf

	; Drive the opening stage here rather than from the auto state's OnBeginState.
	StartSwarmStage(animEventHealthFull)
	ArmHitWatch()

	If AliveTimeoutEnabled
		Float aliveMax = AliveTimeMax
		If aliveMax < AliveTimeMin
			aliveMax = AliveTimeMin
		EndIf
		StartTimer(Utility.RandomFloat(AliveTimeMin, aliveMax), aliveTimerID)
	EndIf
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, bool abPowerAttack, bool abSneakAttack, bool abBashAttack, bool abHitBlocked, string apMaterial)
	ArmHitWatch()
	If hitLock
		Return
	EndIf

	hitLock = True
	EvaluateHealthStage()
	hitLock = False
EndEvent

Event OnDying(Actor akKiller)
	CancelTimer(aliveTimerID)
	CancelTimer(disableTimerID)
	If GetState() != "death"
		GoToState("death")
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == aliveTimerID
		If selfActorRef != None && !selfActorRef.IsDead()
			GoToState("disperse")
			StartTimer(disableWaitTime, disableTimerID)
		EndIf
	ElseIf aiTimerID == disableTimerID
		If selfActorRef != None && !selfActorRef.IsDead()
			selfActorRef.DisableNoWait()
		EndIf
	EndIf
EndEvent
