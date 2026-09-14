; Single-player substitute for the FO76 server round-trip.
; FO4's VM has no SendRMIToServer, so the client request resolves the state
; locally from the owning player's ChangeWaywardStates actor value instead.
Function RequestEventState(Actor source)
	Actor targetPlayer = source
	If targetPlayer == None
		targetPlayer = Game.GetPlayer()
	EndIf
	If targetPlayer == None || W05_MQ_004P_ChangeWaywardStates == None
		Return
	EndIf

	Int stateValue = targetPlayer.GetValue(W05_MQ_004P_ChangeWaywardStates) as Int
	Self.ClientSetEventState(stateValue >= CONST_WaywardStateChangeCompletedValue)
EndFunction

Function ClientRequestEventState()
	Self.RequestEventState(Game.GetPlayer())
EndFunction
