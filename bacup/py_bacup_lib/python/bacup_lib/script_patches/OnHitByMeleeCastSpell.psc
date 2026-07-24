Event OnEffectStart(Actor akTarget, Actor akCaster)
    RegisterForHitEvent(akTarget)
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    UnregisterForHitEvent(akTarget)
    CancelTimer(TimerID)
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, bool abPowerAttack, bool abSneakAttack, bool abBashAttack, bool abHitBlocked, string apMaterial)
    If !OnCooldown
        Bool isMelee = akSource && (akSource.HasKeyword(WeaponTypeHandtoHand) || akSource.HasKeyword(WeaponTypeMelee1H) || akSource.HasKeyword(WeaponTypeMelee2H) || akSource.HasKeyword(WeaponTypeUnarmed))
        If isMelee && Utility.RandomFloat(0.0, 1.0) <= ChanceToCast
            Actor targetActor = akTarget as Actor
            Actor aggressorActor = akAggressor as Actor
            If IsSelfCast && targetActor
                SpellToCast.Cast(targetActor, targetActor)
            ElseIf !IsSelfCast && aggressorActor
                SpellToCast.Cast(targetActor, aggressorActor)
            EndIf
            If Cooldown > 0.0
                OnCooldown = True
                StartTimer(Cooldown, TimerID)
            EndIf
        EndIf
    EndIf
    RegisterForHitEvent(akTarget)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == TimerID
        OnCooldown = False
    EndIf
EndEvent
