Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If akTarget && akTarget.IsDead()
        SpellOnExplosion.Cast(akTarget, akTarget)
    EndIf
EndEvent
