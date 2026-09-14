Function EN05MCT_ArmTargets()
    If GetCount() > 0
        RegisterForHitEvent(Self)
    EndIf
EndFunction

Event OnAliasInit()
    EN05MCT_ArmTargets()
EndEvent

Event OnAliasShutdown()
    UnregisterForHitEvent(Self)
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, \
    Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    If akAggressor != Game.GetPlayer() || akTarget == None || Find(akTarget) < 0
        ; RegisterForHitEvent delivers a single event, so re-arm before bailing out.
        RegisterForHitEvent(Self)
        Return
    EndIf

    RemoveRef(akTarget)
    If GetCount() > 0
        RegisterForHitEvent(Self)
        Return
    EndIf

    Quest owner = GetOwningQuest()
    If owner != None && owner.IsRunning() && !owner.IsStageDone(iStageToSetOnClear)
        owner.SetStage(iStageToSetOnClear)
    EndIf
EndEvent
