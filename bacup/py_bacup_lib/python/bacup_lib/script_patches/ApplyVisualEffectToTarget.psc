Event OnEffectStart(actor Target, actor Caster)
	Float afDuration
	Float fCurrentEffectDuration
	objectreference FacingObject
	fCurrentEffectDuration = LengthDurationOverride
	; FO76 Actor.IsLocalPlayer() -> single-player identity test.
	If Target == Game.GetPlayer()
		If LengthDurationOverride == -1.0
			fCurrentEffectDuration = afDuration
		EndIf
		FacingObject = None
		If !(FacingOverrideRef == None)
			FacingObject = FacingOverrideRef
		EndIf
		EffectToApply.Play(Target as objectreference, fCurrentEffectDuration, FacingObject)
		If !(SoundToPlay == None)
			If !(SoundTriggerDelay == -1.0)
				utility.Wait(SoundTriggerDelay)
			EndIf
			SoundToPlay.Play(Target as objectreference)
		EndIf
	EndIf
EndEvent
