Event OnEffectStart(Actor akTarget, Actor akCaster)
	; Stimpak Diffuser: heal the user, then each living player follower inside the Radius global
	; (FO76 healed the user's team; followers are the single-player team).
	If !HealSpell || !akTarget
		Return
	EndIf
	HealSpell.Cast(akTarget as ObjectReference, akTarget as ObjectReference)
	If !Radius || akTarget != Game.GetPlayer()
		Return
	EndIf
	Float fRadius = Radius.GetValue()
	Actor[] followers = Game.GetPlayerFollowers()
	Int i = 0
	While i < followers.Length
		Actor ally = followers[i]
		If ally && !ally.IsDead() && ally.GetDistance(akTarget as ObjectReference) <= fRadius
			HealSpell.Cast(akTarget as ObjectReference, ally as ObjectReference)
		EndIf
		i += 1
	EndWhile
EndEvent
