Event OnActivate(ObjectReference akActionRef)
	; FO76 SendRMIToServer("ServerRMIFunction") -> direct local call.
	Self.ServerRMIFunction(akActionRef as Actor)
EndEvent
