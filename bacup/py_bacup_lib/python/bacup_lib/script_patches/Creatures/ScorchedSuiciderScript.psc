Function WatchCombatTarget(Actor akTarget)
    If akTarget != None && !IsDead()
        RegisterForDistanceLessThanEvent(Self, akTarget, DistanceToExplode)
    EndIf
EndFunction

Function Detonate()
    If IsDead() || GetState() == "explode"
        Return
    EndIf
    GoToState("explode")
    If DeathExplosion != None
        PlaceAtMe(DeathExplosion)
    EndIf
    Kill()
EndFunction

Event OnLoad()
    WatchCombatTarget(GetCombatTarget())
EndEvent

Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)
    If aeCombatState == 1
        WatchCombatTarget(akTarget)
    EndIf
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    Actor combatTarget = GetCombatTarget()
    If combatTarget != None && (akObj1 == combatTarget || akObj2 == combatTarget)
        Detonate()
    EndIf
EndEvent
