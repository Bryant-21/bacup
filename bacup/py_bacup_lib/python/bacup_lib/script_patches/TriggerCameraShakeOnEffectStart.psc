Event OnEffectStart(actor Target, actor Caster)
	Float afMagnitude
	Float afDuration
	Float fCurrentShakeStrength = ShakeStrengthOverride
	Float fCurrentShakeDuration = ShakeDurationOverride
	; FO76 Actor.IsLocalPlayer() -> single-player identity test.
	If Target == Game.GetPlayer()
		If ShakeStrengthOverride == -1.0
			fCurrentShakeStrength = afMagnitude
		ElseIf ShakeStrengthOverride > 1.0
			fCurrentShakeStrength = 1.0
		EndIf
		If ShakeDurationOverride == -1.0
			fCurrentShakeDuration = afDuration
		EndIf
		If SoundtoPlay
			SoundtoPlay.Play(Target as objectreference)
		EndIf
		; FO76 dispatched the shake through the per-client Player script; FO4 has
		; only the global Game.ShakeCamera.
		Game.ShakeCamera(Target as objectreference, fCurrentShakeStrength, fCurrentShakeDuration)
	EndIf
EndEvent
