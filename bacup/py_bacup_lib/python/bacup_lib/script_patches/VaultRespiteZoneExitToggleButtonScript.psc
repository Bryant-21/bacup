; OnMessageBoxButtonPress is a FO76 event (paired with the asynchronous
; RegisterForMessageBoxPressEvent); Fallout 4 never raises it and its Message.Show()
; is synchronous instead. The confirmation is therefore driven from OnActivate.
; akToggleButtonUser does not exist on FO4's ToggleButtonScript parent, so the
; activating actor is no longer stashed for the parent to consume.
; @drop-member OnMessageBoxButtonPress

State open
	Event OnActivate(ObjectReference akActionRef)
		If akActionRef != Game.GetPlayer()
			Return
		EndIf
		If VaultSystem_RespiteZoneExitConfirmationMessageBox.Show() == 1
			hasConfirmedExit = True
			Self.GoToState("OpenPlayingAnim")
		EndIf
	EndEvent
EndState
