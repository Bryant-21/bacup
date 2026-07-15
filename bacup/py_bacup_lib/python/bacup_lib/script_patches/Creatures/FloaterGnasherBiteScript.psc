Event OnEffectStart(Actor akTarget, Actor akCaster)
	If akCaster != None && !akCaster.IsDead() && VampiricBiteSpell != None
		VampiricBiteSpell.Cast(akCaster, akCaster)
	EndIf
EndEvent
