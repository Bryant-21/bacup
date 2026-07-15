Function RegisterWendigoAnimationEvents()
	If selfRef != None
		RegisterForAnimationEvent(selfRef, animEventScreamAttackStart)
	EndIf
EndFunction

Function UnregisterWendigoAnimationEvents()
	If selfRef != None
		UnregisterForAnimationEvent(selfRef, animEventScreamAttackStart)
	EndIf
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfRef = akCaster
	If selfRef == None
		selfRef = akTarget
	EndIf
	If selfRef != None && !selfRef.IsDead()
		RegisterWendigoAnimationEvents()
	EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If selfRef == None || selfRef.IsDead() || akSource != selfRef || asEventName != animEventScreamAttackStart || ScreamAttackExplosion == None
		Return
	EndIf

	If sExplosionSpawnLocation != ""
		selfRef.PlaceAtNode(sExplosionSpawnLocation, ScreamAttackExplosion)
	Else
		selfRef.PlaceAtMe(ScreamAttackExplosion)
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	UnregisterWendigoAnimationEvents()
	selfRef = None
EndEvent
