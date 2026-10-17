Event OnEffectStart(Actor akTarget, Actor akCaster)
	If !akTarget || !RandomSpells || RandomSpells.Length == 0
		Return
	EndIf
	spell chosen = RandomSpells[Utility.RandomInt(0, RandomSpells.Length - 1)]
	If chosen
		chosen.Cast(akTarget as ObjectReference, akTarget as ObjectReference)
	EndIf
EndEvent
