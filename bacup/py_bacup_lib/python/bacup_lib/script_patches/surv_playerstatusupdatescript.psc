Function TryToShowMessage(ThresholdDatum[] ThresholdDataToSearch, actorvalue ThresholdAV)
	ThresholdDatum foundThresholDatum = Self.GetThresholdDatum(ThresholdDataToSearch, ThresholdAV)
	message MessageToDisplay = None
	If foundThresholDatum
		MessageToDisplay = foundThresholDatum.MessageToDisplay
	EndIf
	If MessageToDisplay
		; FO76 Message.ShowSingle(receiver, ...) -> FO4 Message.Show(); FO4 always
		; targets the single player, so the receiver argument is dropped.
		MessageToDisplay.Show()
	EndIf
EndFunction

Event OnInit()
	; FO76 RegisterForPlayerConnectionEvent(players) + OnPlayerConnected started the
	; survival status ticker when the player logged in. Fallout 4 has no connection
	; lifecycle, so the ticker starts at script init instead.
	Self.StartTimer(TimerDuration, 0)
EndEvent

; @drop-member OnPlayerConnected
