Event OnEffectFinish(Actor akTarget, Actor akCaster)
	CancelTimer(iSoundTimerID)
	CancelTimer(iSpawnShadowTimerID)
	UnregisterForAllRemoteEvents()
	If ImageSpaceToApply
		ImageSpaceToApply.Remove()
	EndIf
EndEvent
