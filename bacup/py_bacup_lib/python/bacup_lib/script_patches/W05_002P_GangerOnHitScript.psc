Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, bool abPowerAttack, bool abSneakAttack, bool abBashAttack, bool abHitBlocked, string apMaterial)
    If OwningPlayer != None && akAggressor == OwningPlayer.GetReference()
        SetStage(StageToSet)
    EndIf
EndEvent
