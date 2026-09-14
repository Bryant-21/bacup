Function ClientRequestEventState()
	; FO76 SendRMIToServer("RequestEventState") invoked the server-authoritative
	; function by name. In single-player the "server" is this machine, so the
	; target function is called directly with the local player as the caller.
	Self.RequestEventState(Game.GetPlayer())
EndFunction
