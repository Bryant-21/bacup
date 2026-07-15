Event OnLoad()
    If SendEventRadius > 0.0
        RegisterForDistanceLessThanEvent(Self, Game.GetPlayer(), SendEventRadius)
    EndIf
EndEvent

Event OnUnload()
    UnregisterForDistanceEvents(Self, Game.GetPlayer())
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    If akObj1 != Self || akObj2 != Game.GetPlayer()
        Return
    EndIf

    SendConfiguredStoryEvent()
    If ResetRadius > SendEventRadius
        RegisterForDistanceGreaterThanEvent(Self, Game.GetPlayer(), ResetRadius)
    EndIf
EndEvent

Event OnDistanceGreaterThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    If akObj1 == Self && akObj2 == Game.GetPlayer() && SendEventRadius > 0.0
        RegisterForDistanceLessThanEvent(Self, Game.GetPlayer(), SendEventRadius)
    EndIf
EndEvent
