Event OnEffectStart(actor akTarget, actor akCaster)
	; FO76 IsAPlayer() -> single-player identity test.
	If akTarget.IsDead() && !akTarget.IsEssential() && akTarget != Game.GetPlayer() && akTarget.IsHostileToActor(akCaster)
		; FO4 Actor.Dismember has no explosion parameter, so the confetti explosion
		; is placed on the target explicitly instead of being passed to Dismember.
		If ExplosionConfettiBalloon != None
			akTarget.PlaceAtMe(ExplosionConfettiBalloon as form, 1, False, False, True)
		EndIf
		akTarget.Dismember("Torso", True, True, True)
	EndIf
EndEvent
