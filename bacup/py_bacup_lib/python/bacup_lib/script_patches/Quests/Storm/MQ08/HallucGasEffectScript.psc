; FO4 has no OnEffectFinishShared, and a non-native script cannot declare a new event.
; @drop-member OnEffectFinishShared
Event OnEffectFinish(Actor akTarget, Actor akCaster)
	CancelTimer(iSoundTimerID)
	CancelTimer(iSpawnShadowTimerID)
	UnregisterForAllRemoteEvents()
	If ImageSpaceToApply
		ImageSpaceToApply.Remove()
	EndIf
EndEvent
