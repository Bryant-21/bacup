; Single-player substitute for the FO76 server round-trip.
; The swap ref owns one StateIndex; it is visible when the player's
; ChangeWaywardStates value selects that index, or when this ref is flagged to
; persist past the stage that set it. InstantSwapValue skips the staged toggle
; once the state change has completed.
Function CheckPlayerWaywardValue(Actor currentPlayer)
	Actor targetPlayer = currentPlayer
	If targetPlayer == None
		targetPlayer = Game.GetPlayer()
	EndIf
	If targetPlayer == None || W05_MQ_004P_ChangeWaywardStates == None
		Return
	EndIf

	Float playerStateValue = targetPlayer.GetValue(W05_MQ_004P_ChangeWaywardStates)
	Bool bEnable = (playerStateValue as Int) == StateIndex
	If !bEnable && MaintainPresenceAfterStageSet && MaintainStateValue >= 0.0
		bEnable = playerStateValue >= MaintainStateValue
	EndIf

	Float toggleLength = ToggleOnTimer
	If playerStateValue >= InstantSwapValue
		toggleLength = -1.0
	EndIf

	Self.UpdateClientEnableState(bEnable, toggleLength, playerStateValue)
EndFunction

Event OnCellLoad()
	Self.CheckPlayerWaywardValue(Game.GetPlayer())
EndEvent
