; The FO76 records bind a hit spell per bear-trap variant. Use the local trigger
; volume to apply it, then let FO4's two-state parent perform the close animation.

Function TriggerBearTrap(ObjectReference actionRef)
    Actor targetActor = actionRef as Actor
    If targetActor != None && isOpen && !isAnimating
        If TrapBearTrapHitSpell != None
            TrapBearTrapHitSpell.Cast(Self, targetActor)
        EndIf
        If FireSound != None && Is3DLoaded()
            FireSound.Play(Self)
        EndIf
        SetOpen(False)
    EndIf
EndFunction

Event OnTriggerEnter(ObjectReference akActionRef)
    TriggerBearTrap(akActionRef)
EndEvent

Event OnActivate(ObjectReference akActionRef)
    TriggerBearTrap(akActionRef)
EndEvent

Event OnLoad()
    parent.OnLoad()
EndEvent

Event OnReset()
    parent.OnReset()
EndEvent
