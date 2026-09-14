; TODO
; Deferred: BumpTumblerUpdateTick() has no caller until the server-side spin driver is restored.

; Re-homed from OnSyncVariableNetworkChanged("tumblerUpdateTick"): FO76 bumped
; tumblerUpdateTick on the server so every client would spin the reels. Exposed as a
; local entry point; SpinReels() itself is unchanged.
Function BumpTumblerUpdateTick()
	tumblerUpdateTick = tumblerUpdateTick + 1
	Self.SpinReels()
EndFunction

; @drop-member OnSyncVariableNetworkChanged
