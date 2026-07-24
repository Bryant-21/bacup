Event OnInit()
    If TrapSystemTargetDistance > 0.0
        TargetDistance = TrapSystemTargetDistance
    EndIf
    If StartsActive
        IsActive = True
        Self.Activate(Self)
    EndIf
EndEvent

Function LocalFireTrap()
    IsActive = True
    StartTimer(ActiveTime, CONST_ActiveTimeTimerID)
    parent.LocalFireTrap()
EndFunction

Function LocalOnTimer(Int aiTimerID)
    If aiTimerID == CONST_ActiveTimeTimerID
        IsActive = False
    EndIf
EndFunction
