; Actor.DispelPotion / ActiveMagicEffect.GetSpell are FO76-only. FO4 can dispel only this
; active effect (Dispel), not every effect of the stimpak potion; the other stimpak
; effects carry their own copy of this script when they need the same behaviour.

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String asMaterialName)
    Dispel()
EndEvent
