; GetDestructibleHealthPercent() > 0 is FO76's "heart not yet destroyed"; FO4's
; equivalent is !IsDestroyed(). OnSyncVariableNetworkChanged (server-to-client replication
; of HeartState/TongueState) is FO76-only and dropped; the alias script calls
; SetHeartStateClient directly and OnLoad replays the current state.
; @drop-member OnSyncVariableNetworkChanged

Function AttackClient()
    If !IsDestroyed()
        PlayAnimation("Attack")
        SetTongueStateClient()
    EndIf
EndFunction

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    If !IsDestroyed()
        PlayAnimation("HitReact")
        RegisterForHitEvent(Self)
    EndIf
EndEvent
