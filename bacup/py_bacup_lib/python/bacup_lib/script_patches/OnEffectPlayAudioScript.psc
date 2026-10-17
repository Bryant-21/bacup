Event OnEffectStart(Actor akTarget, Actor akCaster)
	If StartSound && akTarget
		StartSound.Play(akTarget as ObjectReference)
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	If FinishSound && akTarget
		FinishSound.Play(akTarget as ObjectReference)
	EndIf
EndEvent
