; Re-homed from OnSyncVariableNetworkChanged("bFixed"): FO76 replicated bFixed and
; clients blocked activation. Exposed as a local setter in single-player.
Function SetFixed(Bool abFixed)
	bFixed = abFixed
	If bFixed
		Self.BlockActivation(True, True)
	EndIf
EndFunction

; @drop-member OnSyncVariableNetworkChanged

State Ready
	Event OnActivate(ObjectReference akActionRef)
		Self.GotoState("Busy")
		; FO76 IsAPlayer() -> single-player identity test. FO76's asynchronous
		; RegisterForMessageBoxPressEvent + Message.ShowSingle(receiver, ...) pair
		; collapses to FO4's synchronous Message.Show(), which returns the button
		; index directly. NOTE: the converted script has no surviving handler for
		; the button result, so the choice is still discarded.
		If akActionRef == Game.GetPlayer() && RepairMessage != None
			RepairMessage.Show()
		EndIf
		Self.GotoState("Ready")
	EndEvent
EndState
