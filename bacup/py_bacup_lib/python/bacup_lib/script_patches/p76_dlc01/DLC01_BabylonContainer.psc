State open
	Event OnBeginState(String asOldState)
		; FO76 ObjectReference.SetOpenState(open, playSound) -> FO4 SetOpen(open).
		; The FO76 "suppress sound" flag has no FO4 counterpart.
		Self.SetOpen(True)
	EndEvent
EndState
