Function RegisterWatcherDistance()
	If selfActorRef == None
		Return
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef != None && RangeToDisappear > 0.0
		RegisterForDistanceLessThanEvent(selfActorRef, playerRef, RangeToDisappear)
	EndIf
EndFunction

Function UnregisterWatcherEvents()
	If selfActorRef == None
		Return
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForDistanceEvents(selfActorRef, playerRef)
	EndIf
	UnregisterForAnimationEvent(selfActorRef, animEventTeleportStart)
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfActorRef = akCaster
	If selfActorRef == None
		selfActorRef = akTarget
	EndIf
	If selfActorRef != None && !selfActorRef.IsDead()
		RegisterWatcherDistance()
	EndIf
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
	If selfActorRef != None && !selfActorRef.IsDead()
		GoToState("disappear")
	EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If selfActorRef != None && !selfActorRef.IsDead() && akSource == selfActorRef && asEventName == animEventTeleportStart
		If DisappearExplosion != None
			selfActorRef.PlaceAtMe(DisappearExplosion)
		EndIf
		selfActorRef.Disable()
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	CancelTimerGameTime(sunriseTimerID)
	UnregisterWatcherEvents()
	selfActorRef = None
EndEvent
