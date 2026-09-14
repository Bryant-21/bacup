Event OnLooksMenuEvent(Int aiFlavor)
	If allowLooksMenuComment
		; FO76 SendRMIToServer("ServerPlayLooksMenuEventComment") invoked the
		; server-authoritative function by name. In single-player the "server" is
		; this machine, so the target is called directly.
		Self.ServerPlayLooksMenuEventComment(None)
	EndIf
EndEvent

; OnSyncVariableNetworkChanged replicated myBarberChairPlayer/allowLooksMenuComment.
; The surviving body only re-registered for looks-menu events, which is exposed here.
Function RegisterForBarberChairLooksMenu()
	Self.RegisterForLooksMenuEvent()
EndFunction

; @drop-member OnSyncVariableNetworkChanged
