Event OnQuestInit()
	CancelTimer(93187)
	; Stage 500/650/800 log entries select their fragment by this global (1 = Strangler Queen, 2 = Grafton Monster).
	If SFS08_Heart_BossGlobal != None
		SFS08_Heart_BossGlobal.SetValue(Utility.RandomInt(1, 2) as Float)
	EndIf
	Quest owner = Self as Quest
	B21:QuestTimer questTimer = owner as B21:QuestTimer
	If questTimer != None
		RegisterForCustomEvent(questTimer, "QuestTimerEnded")
	EndIf
EndEvent

Event B21:QuestTimer.QuestTimerEnded(B21:QuestTimer akSender, Var[] akArgs)
	If IsRunning() && IsStageDone(100) && !IsStageDone(1000) && !IsStageDone(1100)
		SetStage(1100)
	EndIf
EndEvent

; Delayed so stage 1000/1100 reward and game-days handlers run before the quest stops.
Function ScheduleEventShutdown()
	CancelTimer(93187)
	StartTimer(10.0, 93187)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 93187 && IsRunning()
		Stop()
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(93187)
	Quest owner = Self as Quest
	B21:QuestTimer questTimer = owner as B21:QuestTimer
	If questTimer != None
		UnregisterForCustomEvent(questTimer, "QuestTimerEnded")
	EndIf
EndEvent
