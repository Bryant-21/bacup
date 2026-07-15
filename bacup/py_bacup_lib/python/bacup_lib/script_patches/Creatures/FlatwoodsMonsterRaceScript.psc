Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfRef = akCaster
	If selfRef == None
		selfRef = akTarget
	EndIf
	If selfRef != None && !selfRef.IsDead()
		RegisterForAnimationEvent(selfRef, animEventTeleportVFX)
	EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If selfRef != None && !selfRef.IsDead() && akSource == selfRef && asEventName == animEventTeleportVFX && TeleportOutSpell != None
		TeleportOutSpell.Cast(selfRef, selfRef)
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	If selfRef != None
		UnregisterForAnimationEvent(selfRef, animEventTeleportVFX)
	EndIf
	selfRef = None
EndEvent
