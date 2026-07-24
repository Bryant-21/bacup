Event OnEffectStart(Actor akTarget, Actor akCaster)
    EffectOwner = akTarget
    RegisterForHitEvent(akTarget)
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    UnregisterForHitEvent(akTarget)
    CancelTimer(1)
EndEvent

; RegisterForHitEvent is single-shot, so every branch below must end by
; re-registering (immediately for a non-qualifying hit, or after the cooldown
; timer for a qualifying one) - there is no separate "on cooldown" flag declared
; on this skeleton, so the register/timer cycle itself models the cooldown state.
Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, bool abPowerAttack, bool abSneakAttack, bool abBashAttack, bool abHitBlocked, string apMaterial)
    If akSource && (akSource.HasKeyword(unarmedKeyword) || akSource.HasKeyword(meleeKeyword))
        If BurnSelfSpell && EffectOwner
            BurnSelfSpell.Cast(EffectOwner, EffectOwner)
        EndIf
        Actor aggressorActor = akAggressor as Actor
        If BurnAttackerSpell && aggressorActor
            BurnAttackerSpell.Cast(EffectOwner, aggressorActor)
        EndIf
        StartTimer(SpellCooldown, 1)
    Else
        RegisterForHitEvent(EffectOwner)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1 && EffectOwner
        RegisterForHitEvent(EffectOwner)
    EndIf
EndEvent
