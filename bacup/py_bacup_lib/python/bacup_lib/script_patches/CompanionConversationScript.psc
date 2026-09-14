Event OnLoad()
	If IsDead()
		AllyCleanedUp = True
		CancelTimer(1)
		Return
	EndIf
	AllyCleanedUp = False
	CancelTimer(1)
	StartTimer(TimerDuration, 1)
EndEvent

Event OnUnload()
	CancelTimer(1)
EndEvent

Event OnDeath(Actor akKiller)
	AllyCleanedUp = True
	CancelTimer(1)
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != 1 || AllyCleanedUp
		Return
	EndIf

	If COMP_ConversationStart && (!COMP_AV_VisitorWaiting || GetValue(COMP_AV_VisitorWaiting) <= 0.0)
		COMP_ConversationStart.SendStoryEventAndWait(GetCurrentLocation(), Self)
	EndIf
	StartTimer(TimerDuration, 1)
EndEvent
