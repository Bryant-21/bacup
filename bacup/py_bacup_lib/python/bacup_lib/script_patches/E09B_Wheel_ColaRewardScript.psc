Event OnEffectStart(Actor akTarget, Actor akCaster)
    If Effects == None || Effects.Length == 0
        Return
    EndIf
    Spell chosen = Effects[Utility.RandomInt(0, Effects.Length - 1)]
    If chosen != None
        chosen.Cast(akTarget, akTarget)
    EndIf
EndEvent
