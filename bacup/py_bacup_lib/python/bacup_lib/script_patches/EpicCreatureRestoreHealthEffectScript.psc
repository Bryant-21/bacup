Event OnEffectStart(Actor akTarget, Actor akCaster)
    If !akTarget
        Return
    EndIf

    SpellToCast.Cast(akTarget, akTarget)
    If PowerUpSound
        PowerUpSound.Play(akTarget)
    EndIf
    If LegendaryPowerUpMsg
        LegendaryPowerUpMsg.Show()
    EndIf

    ; AbilityToRemove is the ability spell hosting this very effect - removing it
    ; here is the one-shot "enrage/power-up" guard so it cannot re-trigger without
    ; being explicitly re-applied by whatever grants it.
    akTarget.RemoveSpell(AbilityToRemove)
EndEvent
