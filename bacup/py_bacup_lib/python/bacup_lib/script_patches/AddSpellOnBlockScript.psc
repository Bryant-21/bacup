Event OnEffectStart(Actor akTarget, Actor akCaster)
	If akTarget
		Self.RegisterForHitEvent(akTarget as ScriptObject, None, None, None, -1, -1, -1, -1, True)
	EndIf
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	; The legendary "on block" effects punish the attacker whose blow was blocked.
	Actor attacker = akAggressor as Actor
	If abHitBlocked && SpellToCast && attacker && !attacker.IsDead()
		If ChanceToCast >= 1.0 || Utility.RandomFloat(0.0, 1.0) < ChanceToCast
			SpellToCast.Cast(akTarget, attacker as ObjectReference)
		EndIf
	EndIf
	If akTarget && !(akTarget as Actor).IsDead()
		Self.RegisterForHitEvent(akTarget as ScriptObject, None, None, None, -1, -1, -1, -1, True)
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	Self.UnregisterForAllHitEvents(None)
EndEvent
