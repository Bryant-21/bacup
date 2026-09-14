; Re-homed from OnSyncVariableNetworkChanged("Count"): FO76 replicated Count from the
; server and clients ran the transition animation. Single-player has no replication,
; so the setter is exposed locally and runs the same transition.
Function SetCount(Int iNewCount)
	If Count == iNewCount
		Return
	EndIf
	Count = iNewCount
	If clientLastCount != Count
		Self.CheckMaxCountClient(Count)
		clientLastCount = Count
	EndIf
EndFunction

; @drop-member OnSyncVariableNetworkChanged
