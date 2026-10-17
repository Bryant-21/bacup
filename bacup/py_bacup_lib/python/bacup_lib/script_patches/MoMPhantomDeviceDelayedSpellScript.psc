Event OnEffectStart(Actor akTarget, Actor akCaster)
	user = akTarget
	If user && DelayedSpell
		Self.StartTimer(TimeDelay, Timer_TimeDelayTimerID)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == Timer_TimeDelayTimerID && user && DelayedSpell && !user.IsDead()
		DelayedSpell.Cast(user as ObjectReference, user as ObjectReference)
	EndIf
EndEvent
