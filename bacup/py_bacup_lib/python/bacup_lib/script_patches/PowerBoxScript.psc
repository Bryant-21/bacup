; Re-homed from OnSyncVariableNetworkChanged("isActive"): FO76 replicated isActive
; from the server and clients re-ran the animation update. Single-player has no
; replication, so the setter is exposed locally for the owning quest/script to call.
Function SetPowerBoxActive(Bool bActive)
	If isActive == bActive
		Return
	EndIf
	isActive = bActive
	Self.ClientUpdatePowerBoxAnimationState()
EndFunction

; @drop-member OnSyncVariableNetworkChanged
