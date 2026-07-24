Event OnEffectStart(Actor akTarget, Actor akCaster)
    EffectOwner = akTarget
    RegisterForHitEvent(EffectOwner)
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    CancelTimer(iCooldownTimerID)
    UnregisterForHitEvent(EffectOwner)
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    If Utility.RandomInt(1, 100) <= ExplosionChance
        If ExplosionForm
            EffectOwner.PlaceAtMe(ExplosionForm)
        EndIf
        StartTimer(ExplosionCooldown, iCooldownTimerID)
    Else
        StartTimer(FailCooldown, iCooldownTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iCooldownTimerID
        RegisterForHitEvent(EffectOwner)
    EndIf
EndEvent
