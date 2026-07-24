Event OnEffectStart(Actor akTarget, Actor akCaster)
    akTarget.AddKeyword(NWOT_Fortune_BookKeyword)
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    akTarget.RemoveKeyword(NWOT_Fortune_BookKeyword)
EndEvent
