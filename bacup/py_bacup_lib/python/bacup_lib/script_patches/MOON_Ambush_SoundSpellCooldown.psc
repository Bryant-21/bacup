Event OnEffectStart(Actor akTarget, Actor akCaster)
    akTarget.AddKeyword(MOON_Ambush_Keyword_SoundCooldown)
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    akTarget.RemoveKeyword(MOON_Ambush_Keyword_SoundCooldown)
EndEvent
