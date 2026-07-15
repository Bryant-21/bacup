; The separate PhysicalTrapHit script owns victim damage. This script owns only
; the board's bound fire animation and optional self-damage payload.

Function ClientFireTrap()
    PhysicalTrapHit hitScript = Self as PhysicalTrapHit
    If hitScript != None
        hitScript.SetCanHit(True)
    EndIf

    String fireAnimation = FireTrapAnim
    String fireEvent = FireTrapAnimEndEvent
    If fireAnimation == ""
        fireAnimation = "Trip"
    EndIf
    If fireEvent == ""
        fireEvent = "TransitionComplete"
    EndIf
    PlayAnimationAndWait(fireAnimation, fireEvent)

    If hitScript != None
        hitScript.SetCanHit(False)
    EndIf
    If LocalSelfDamage > 0.0
        DamageObject(LocalSelfDamage)
    EndIf
    GoToState("fired")
EndFunction
