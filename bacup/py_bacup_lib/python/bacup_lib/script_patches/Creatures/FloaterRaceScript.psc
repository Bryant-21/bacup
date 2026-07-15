Event OnEffectStart(Actor akTarget, Actor akCaster)
	Actor floater = akCaster
	If floater == None
		floater = akTarget
	EndIf
	If floater != None && !floater.IsDead()
		RegisterForAnimationEvent(floater, animEventAttack)
	EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	Actor floater = akSource as Actor
	If floater != None && !floater.IsDead() && asEventName == animEventAttack && VampiricBiteSpell != None
		VampiricBiteSpell.Cast(floater, floater)
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	Actor floater = akCaster
	If floater == None
		floater = akTarget
	EndIf
	If floater != None
		UnregisterForAnimationEvent(floater, animEventAttack)
	EndIf
EndEvent
