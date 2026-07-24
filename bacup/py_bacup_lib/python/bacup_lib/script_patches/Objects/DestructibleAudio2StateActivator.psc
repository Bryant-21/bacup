Function SetOpen(Bool abOpen = True)
	If abOpen
		Self.CancelTimer(TimerID_SoundOffDelay)
	EndIf

	parent.SetOpen(abOpen)

	If abOpen
		If !Self.IsDestroyed() && Self.Is3DLoaded()
			Self.PlaySoundOnClient()
		EndIf
	ElseIf Delay > 0.0
		Self.StartTimer(Delay, TimerID_SoundOffDelay)
	Else
		Self.StopSoundOnClient()
	EndIf
EndFunction

Event OnLoad()
	parent.OnLoad()
	If Self.IsDestroyed()
		Self.StopSoundOnClient()
	ElseIf Self.Is3DLoaded() && isOpen
		Self.PlaySoundOnClient()
	EndIf
EndEvent

Event OnReset()
	parent.OnReset()
	If Self.IsDestroyed()
		Self.StopSoundOnClient()
	ElseIf Self.Is3DLoaded() && isOpen
		Self.PlaySoundOnClient()
	Else
		Self.StopSoundOnClient()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == TimerID_SoundOffDelay
		Self.StopSoundOnClient()
	Else
		parent.OnTimer(aiTimerID)
	EndIf
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
	If aiCurrentStage > 0
		Self.StopSoundOnClient()
	ElseIf isOpen && Self.Is3DLoaded()
		Self.PlaySoundOnClient()
	EndIf
EndEvent
