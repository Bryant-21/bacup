Function RegisterWatcherDistance()
	If selfRef == None
		Return
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef != None && RangeToDisappear > 0.0
		RegisterForDistanceLessThanEvent(selfRef, playerRef, RangeToDisappear)
	EndIf
EndFunction

Function UnregisterWatcherEvents()
	If selfRef == None
		Return
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForDistanceEvents(selfRef, playerRef)
	EndIf
	UnregisterForAnimationEvent(selfRef, animEventTeleportStart)
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfRef = akCaster
	If selfRef == None
		selfRef = akTarget
	EndIf
	If selfRef != None && !selfRef.IsDead()
		RegisterWatcherDistance()
	EndIf
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
	If selfRef != None && !selfRef.IsDead()
		GoToState("disappear")
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	UnregisterWatcherEvents()
	selfRef = None
EndEvent
