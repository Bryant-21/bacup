Event OnLoad()
	; FO76 ObjectReference.IsReserved() reported a per-player instanced-container
	; reservation held by the server. Single-player has no reservations, so the
	; container is always unlocked here.
	Self.Lock(False, False)
EndEvent
