Event OnEffectFinish(Actor akTarget, Actor akCaster)
	; The *_Duration effects hand off to their follow-up spell (hangover, bounty cloak, candy crash) when they expire.
	If SpellToAdd && akTarget && !akTarget.IsDead()
		SpellToAdd.Cast(akTarget as ObjectReference, akTarget as ObjectReference)
	EndIf
EndEvent
