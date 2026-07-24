Event OnEffectStart(Actor akTarget, Actor akCaster)
    EffectOwner = akTarget
    RegisterForHitEvent(akTarget)
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    UnregisterForHitEvent(akTarget)
    CancelTimer(iCooldownTimerID)
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, bool abPowerAttack, bool abSneakAttack, bool abBashAttack, bool abHitBlocked, string apMaterial)
    Bool isMelee = akSource && (akSource.HasKeyword(WeaponTypeMelee1H) || akSource.HasKeyword(WeaponTypeMelee2H) || akSource.HasKeyword(WeaponTypeUnarmed))
    If !isMelee
        RegisterForHitEvent(EffectOwner)
        Return
    EndIf

    If Utility.RandomFloat(0.0, 1.0) <= ExplosionChance.GetValue()
        Actor aggressorActor = akAggressor as Actor
        If aggressorActor
            aggressorActor.PlaceAtMe(ExplosionForm)
        EndIf
        EffectOwner.DamageValue(Health, SelfDamage.GetValue())
        StartTimer(ExplosionCooldown.GetValue(), iCooldownTimerID)
    Else
        StartTimer(FailCooldown.GetValue(), iCooldownTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iCooldownTimerID && EffectOwner
        RegisterForHitEvent(EffectOwner)
    EndIf
EndEvent
