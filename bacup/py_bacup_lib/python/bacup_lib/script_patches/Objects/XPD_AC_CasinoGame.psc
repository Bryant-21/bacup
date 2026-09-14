; TODO
; Deferred: SetResultAnimationIdx() has no caller until the server-side result driver is restored.

; Re-homed from OnSyncVariableNetworkChanged("ResultAnimationIdx"): FO76 replicated
; the result index and clients played the reveal. Exposed as a local setter.
Function SetResultAnimationIdx(Int aiResultIndex)
	ResultAnimationIdx = aiResultIndex
	If ResultAnimationIdx != ResultIndexResetValue
		Self.SetAnimationVariableInt(AnimResultVarName, ResultAnimationIdx)
		Self.PlayAnimation(PlayAnim)
	EndIf
EndFunction

; @drop-member OnSyncVariableNetworkChanged
