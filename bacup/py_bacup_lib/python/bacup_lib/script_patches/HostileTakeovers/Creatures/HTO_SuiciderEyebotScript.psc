Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfRef = akTarget
	bValidSuicider = selfRef != None && ActorTypeSuicider != None && selfRef.HasKeyword(ActorTypeSuicider)
	LocalTrace("OnEffectStart valid=" + bValidSuicider, 0)
	If bValidSuicider && selfRef.IsInCombat()
		StartTimer(0.5)
	EndIf
EndEvent

Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)
	If !bValidSuicider
		Return
	EndIf
	If aeCombatState == 1
		StartTimer(0.5)
	Else
		CancelTimer()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If !bValidSuicider || selfRef == None || selfRef.IsDead()
		Return
	EndIf
	Actor combatTarget = selfRef.GetCombatTarget()
	If combatTarget != None && selfRef.GetDistance(combatTarget) <= DistanceToExplode
		GoToState("explode")
		LocalTrace("detonating on " + combatTarget, 0)
		If DeathExplosion != None
			selfRef.PlaceAtMe(DeathExplosion)
		EndIf
		selfRef.Kill()
	ElseIf selfRef.IsInCombat()
		StartTimer(0.5)
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	CancelTimer()
EndEvent

State explode
	Event OnTimer(Int aiTimerID)
	EndEvent

	Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)
	EndEvent
EndState
