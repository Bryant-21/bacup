Function ArmStageTimer()
	Float delay = TimerDuration
	If delay < 1.0
		delay = 1.0
	EndIf
	CancelTimer(87531)
	StartTimer(delay, 87531)
EndFunction

Event OnQuestInit()
	If StartTimerOnInit
		ArmStageTimer()
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If StageToStartTimer >= 0 && auiStageID == StageToStartTimer
		ArmStageTimer()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 87531 && IsRunning() && StageToSetOnTimerEnd >= 0 && !IsStageDone(StageToSetOnTimerEnd)
		SetStage(StageToSetOnTimerEnd)
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(87531)
EndEvent
