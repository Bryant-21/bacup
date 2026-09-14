Event OnLoad()
    If Is3DLoaded() && !IsDestroyed()
        RegisterForHitEvent(Self)
    EndIf
EndEvent

Event OnUnload()
    UnregisterForAllHitEvents()
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    DamageObject(Damage)

    If !IsDestroyed()
        RegisterForHitEvent(Self)
    EndIf
EndEvent
