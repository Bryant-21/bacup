; Local hit/miss handling; no RMI or multiplayer sender contract.

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, Bool abCritAttack, String asMaterialName)
    If isHit
        Return
    EndIf
    isHit = True
    HitAnimation()
    If GameController != None
        GameController.RegisterTargetHit(scoreOnHit)
    EndIf
    Utility.Wait(0.3)
    Delete()
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
    If asEventName == "Done"
        If !isHit && GameController != None
            GameController.RegisterTargetMiss()
        EndIf
        Utility.Wait(0.3)
        Delete()
    EndIf
EndEvent

Function HitAnimation()
    If hitSFX != None
        hitSFX.Play(Self)
    EndIf
    If onHitAnim != ""
        PlayAnimation(onHitAnim)
    EndIf
EndFunction
