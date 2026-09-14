Function StartShutdownTimer()
	CancelTimer(myTimerID)
	Float shutdownDelay = Shutdown_Timer as Float
	If shutdownDelay < 1.0
		shutdownDelay = 1.0
	EndIf
	StartTimer(shutdownDelay, myTimerID)
EndFunction

Function CancelShutdownTimer()
	CancelTimer(myTimerID)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == myTimerID && IsRunning() && !IsStageDone(1600)
		SetStage(1600)
	EndIf
EndEvent
