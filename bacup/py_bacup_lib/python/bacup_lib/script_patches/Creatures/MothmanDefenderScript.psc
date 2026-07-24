Function RegisterDefenderEvents()
	If selfActorRef == None
		Return
	EndIf

	RegisterForRemoteEvent(selfActorRef, "OnCombatStateChanged")
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && RangeToGoIntoCombat > 0.0
		RegisterForDistanceLessThanEvent(selfActorRef, playerRef, RangeToGoIntoCombat)
	EndIf
EndFunction

Function UnregisterDefenderEvents()
	If selfActorRef == None
		Return
	EndIf

	UnregisterForRemoteEvent(selfActorRef, "OnCombatStateChanged")
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForDistanceEvents(selfActorRef, playerRef)
	EndIf
	UnregisterForAnimationEvent(selfActorRef, animEventTeleportStart)
EndFunction

Function EnterCombatantState()
	If selfActorRef == None || selfActorRef.IsDead()
		Return
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForDistanceEvents(selfActorRef, playerRef)
	EndIf
	GoToState("incombat")
	If MothmanCombatantSpell != None
		selfActorRef.AddSpell(MothmanCombatantSpell, False)
	EndIf
	CancelTimer(disappearTimerID)
	StartTimer(Utility.RandomFloat(DisappearTimeMin, DisappearTimeMax), disappearTimerID)
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfActorRef = akCaster
	If selfActorRef == None
		selfActorRef = akTarget
	EndIf
	If selfActorRef == None || selfActorRef.IsDead()
		Return
	EndIf

	GoToState("Watcher")
	RegisterDefenderEvents()
	If selfActorRef.GetCombatState() == CombatState_InCombat
		EnterCombatantState()
	EndIf
EndEvent

Event Actor.OnCombatStateChanged(Actor akSender, Actor akTarget, Int aeCombatState)
	If akSender == selfActorRef && aeCombatState == CombatState_InCombat
		EnterCombatantState()
	EndIf
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
	If selfActorRef != None && !selfActorRef.IsDead() && GetState() == "Watcher"
		EnterCombatantState()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == disappearTimerID && selfActorRef != None && !selfActorRef.IsDead()
		disappearExplosion = DisappearExplosionTime
		GoToState("disappear")
	EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If selfActorRef != None && !selfActorRef.IsDead() && akSource == selfActorRef && asEventName == animEventTeleportStart
		If disappearExplosion != None
			selfActorRef.PlaceAtMe(disappearExplosion)
		EndIf
		selfActorRef.Disable()
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	CancelTimer(disappearTimerID)
	CancelTimerGameTime(sunriseTimerID)
	UnregisterDefenderEvents()
	If selfActorRef != None && MothmanCombatantSpell != None
		selfActorRef.RemoveSpell(MothmanCombatantSpell)
	EndIf
	selfActorRef = None
EndEvent
