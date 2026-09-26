; B21_TalesFromAppalachia's Casino native runtime now owns the local result driver.
; This setter remains available to non-native callers and isolated script tests.

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
