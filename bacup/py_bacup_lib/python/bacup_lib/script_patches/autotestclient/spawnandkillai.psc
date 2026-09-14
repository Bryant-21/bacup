; FO76 Actor.FindRandomCombatTarget(radius) and Actor.SetAIDriven() have no Fallout 4
; equivalent. BEHAVIOUR LOST: the harness no longer picks a target or drives the bot;
; FightAI() is now inert. The ScriptObject cast on RegisterForRemoteEvent also had to
; go, since OnDying resolves on Actor, not ScriptObject.
Bool Function FightAI()
	Self.StartTimer(time as Float, 0)
	Return False
EndFunction
