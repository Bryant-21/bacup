Event OnEffectStart(Actor akTarget, Actor akCaster)
	NotCombatSoundID = 0
	CombatSoundID = 0
	ScheduleNextSound()
EndEvent

Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)
	Self.CancelTimer(iPlayCombatSoundTimerID)
	StopLoopingSounds()
	ScheduleNextSound()
EndEvent

Event OnTimer(Int aiTimerID)
	Actor target = Self.GetTargetActor()
	If !target || target.IsDead()
		Return
	EndIf
	If target.IsInCombat()
		If PlayInCombat && CombatSound
			PlayCombatSoundOnClient()
		EndIf
	ElseIf PlayNotInCombat && NotCombatSound
		PlayNotCombatSoundOnClient()
	EndIf
	ScheduleNextSound()
EndEvent

Event OnDying(Actor akKiller)
	Self.CancelTimer(iPlayCombatSoundTimerID)
	StopLoopingSounds()
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	Self.CancelTimer(iPlayCombatSoundTimerID)
	StopLoopingSounds()
EndEvent

; Both FO76 timer IDs are 1, so a single timer drives whichever sound matches the current combat state.
Function ScheduleNextSound()
	Actor target = Self.GetTargetActor()
	If !target || target.IsDead()
		Return
	EndIf
	If target.IsInCombat()
		If PlayInCombat && CombatSound
			Self.StartTimer(Utility.RandomFloat(DelayCombatMin, DelayCombatMax), iPlayCombatSoundTimerID)
		EndIf
	ElseIf PlayNotInCombat && NotCombatSound
		Self.StartTimer(Utility.RandomFloat(DelayNotCombatMin, DelayNotCombatMax), iPlayNotCombatSoundTimerID)
	EndIf
EndFunction

Function StopLoopingSounds()
	If NotCombatSoundID > 0
		StopNotCombatSoundOnClient()
		NotCombatSoundID = 0
	EndIf
	If CombatSoundID > 0
		StopCombatSoundOnClient()
		CombatSoundID = 0
	EndIf
EndFunction
