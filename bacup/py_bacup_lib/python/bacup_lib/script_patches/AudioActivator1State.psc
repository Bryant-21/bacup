; FO76 Default1StateActivator names its animation PlayAnim; FO4's calls it Anim.
Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    HitSound.Play(Self)
    PlayAnimation(Anim)
    RegisterForHitEvent(Self)
EndEvent
