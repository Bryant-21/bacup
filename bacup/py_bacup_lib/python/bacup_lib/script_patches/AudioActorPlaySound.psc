Function RefreshSoundTimers()
	StopSoundTimers()

	If IsDead() || !IsEnabled()
		Return
	EndIf

	Int combatState = GetCombatState()
	If combatState == 1 && PlayInCombat && CombatSound != None
		bRunningCombatTimer = True
		StartTimer(Utility.RandomFloat(delayMin, delayMax), iPlayCombatSoundTimerID)
	ElseIf combatState == 0 && PlayNotInCombat && NotCombatSound != None
		bRunningNotCombatTimer = True
		StartTimer(Utility.RandomFloat(delayMin, delayMax), iPlayNotCombatSoundTimerID)
	EndIf
EndFunction

Function StopSoundTimers()
	CancelTimer(iPlayCombatSoundTimerID)
	CancelTimer(iPlayNotCombatSoundTimerID)

	If CombatSoundID > 0
		StopCombatSoundOnClient()
		CombatSoundID = -1
	EndIf
	If NotCombatSoundID > 0
		StopNotCombatSoundOnClient()
		NotCombatSoundID = -1
	EndIf

	bRunningCombatTimer = False
	bRunningNotCombatTimer = False
EndFunction

Event OnLoad()
	CombatSoundID = -1
	NotCombatSoundID = -1
	bRunningCombatTimer = False
	bRunningNotCombatTimer = False
	RefreshSoundTimers()
EndEvent

Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)
	RefreshSoundTimers()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != iPlayCombatSoundTimerID
		Return
	EndIf

	Int combatState = GetCombatState()
	If combatState == 1 && bRunningCombatTimer && PlayInCombat && CombatSound != None
		PlayCombatSoundOnClient()
		StartTimer(Utility.RandomFloat(delayMin, delayMax), iPlayCombatSoundTimerID)
	ElseIf combatState == 0 && bRunningNotCombatTimer && PlayNotInCombat && NotCombatSound != None
		PlayNotCombatSoundOnClient()
		StartTimer(Utility.RandomFloat(delayMin, delayMax), iPlayNotCombatSoundTimerID)
	Else
		StopSoundTimers()
	EndIf
EndEvent

Event OnCellDetach()
	StopSoundTimers()
EndEvent

Event OnUnload()
	StopSoundTimers()
EndEvent

Event OnDying(Actor akKiller)
	StopSoundTimers()
EndEvent
