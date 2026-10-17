; A hallucinated shadow (Storm_MQ08_HallucGas_ShadowSpell ability) vanishes in smoke when it dies, whether killed by
; the player or dissolved when the gas wears off.
Event OnDying(Actor akKiller)
	Actor shadow = GetTargetActor()
	If shadow == None
		Return
	EndIf
	If SmokeExplosion
		shadow.PlaceAtMe(SmokeExplosion)
	EndIf
	shadow.Disable()
	shadow.Delete()
EndEvent
