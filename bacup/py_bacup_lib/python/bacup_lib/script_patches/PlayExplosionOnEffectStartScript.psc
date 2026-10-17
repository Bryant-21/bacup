Event OnEffectStart(Actor akTarget, Actor akCaster)
	If ExplosionToSpawn && akTarget
		akTarget.PlaceAtMe(ExplosionToSpawn as Form, 1, False, False, True)
	EndIf
EndEvent
