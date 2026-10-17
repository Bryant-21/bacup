; Battle Banner kill proc: the killer gets BattleBannerBuffSpell, whose effects are gated on the banner auras they carry.
Event OnEffectStart(Actor akTarget, Actor akCaster)
    Actor killer = akCaster
    If killer == None
        killer = akTarget
    EndIf
    If killer != None && BattleBannerBuffSpell != None
        BattleBannerBuffSpell.Cast(killer, killer)
    EndIf
EndEvent
