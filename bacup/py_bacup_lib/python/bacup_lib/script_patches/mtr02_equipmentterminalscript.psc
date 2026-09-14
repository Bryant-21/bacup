Event OnActivate(ObjectReference akActionRef)
	; FO76 IsLocalPlayer() -> single-player identity test. NOTE: the guarded body
	; is empty in the converted (server-stripped) script.
	If akActionRef == Game.GetPlayer()
	EndIf
EndEvent
