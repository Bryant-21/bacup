; Brutalist's on-kill proc: each kill adds one to NumKillsAV on the killer, plays KillSoundEffect and (re)applies the
; Strength buff; LegendaryBrutalistsFinishedScript clears the count when the buff runs out.
Event OnEffectStart(Actor akTarget, Actor akCaster)
    Actor killer = akCaster
    If killer == None
        killer = akTarget
    EndIf
    If killer == None || BrutalistsBuffSpell == None
        Return
    EndIf
    If NumKillsAV != None
        killer.ModValue(NumKillsAV, 1.0)
    EndIf
    If KillSoundEffect != None
        KillSoundEffect.Play(killer)
    EndIf
    BrutalistsBuffSpell.Cast(killer, killer)
EndEvent
