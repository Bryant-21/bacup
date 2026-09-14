; FO76 ObjectReference.IsLocalPlayer() has no Fallout 4 equivalent. Single-player
; has exactly one player, so the multiplayer local-client guard becomes an
; identity test against Game.GetPlayer(). NOTE: the guarded body is empty in the
; converted (server-stripped) script -- this handler is still behaviourally hollow.
Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
	EndIf
EndEvent
