Function RegisterForPlayerProximity()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && DistanceToExplode > 0.0
		RegisterForDistanceLessThanEvent(Self, playerRef, DistanceToExplode)
	EndIf
EndFunction

Function UnregisterForPlayerProximity()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForDistanceEvents(Self, playerRef)
	EndIf
EndFunction

Event OnLoad()
	If GetCombatState() == 1
		RegisterForPlayerProximity()
	EndIf
EndEvent

Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)
	If aeCombatState == 1
		RegisterForPlayerProximity()
	Else
		UnregisterForPlayerProximity()
	EndIf
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
	If !IsDead() && GetCombatState() == 1
		UnregisterForPlayerProximity()
		GoToState("explode")
		If DeathExplosion != None
			PlaceAtMe(DeathExplosion)
		EndIf
		Kill()
	EndIf
EndEvent

Event OnDeath(Actor akKiller)
	UnregisterForPlayerProximity()
EndEvent

Event OnUnload()
	UnregisterForPlayerProximity()
EndEvent
