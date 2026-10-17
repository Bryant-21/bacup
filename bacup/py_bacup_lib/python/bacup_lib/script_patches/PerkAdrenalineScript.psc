; Adrenaline / Battle Charged on-kill proc: each kill adds one to NumKillsAV on the killer and (re)applies
; AdrenalineBuffSpell; PerkAdrenalineFinishedScript clears the count when the buff runs out.
Event OnEffectStart(Actor akTarget, Actor akCaster)
    Actor killer = akCaster
    If killer == None
        killer = akTarget
    EndIf
    If killer == None || AdrenalineBuffSpell == None
        Return
    EndIf
    If NumKillsAV != None
        killer.ModValue(NumKillsAV, 1.0)
    EndIf
    AdrenalineBuffSpell.Cast(killer, killer)
EndEvent
