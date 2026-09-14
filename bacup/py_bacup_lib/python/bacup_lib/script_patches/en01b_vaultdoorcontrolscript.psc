State Ready
	Event OnActivate(ObjectReference akActionRef)
		Self.GotoState("Busy")
		; FO76 ObjectReference.IsLocalPlayer() has no Fallout 4 equivalent; in
		; single-player the activating reference is compared against the player.
		; NOTE: the guarded body is empty in the converted (server-stripped)
		; script, so this handler remains behaviourally hollow.
		If akActionRef == Game.GetPlayer()
		EndIf
		Self.GotoState("Ready")
	EndEvent
EndState
