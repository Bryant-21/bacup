Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget == Game.GetPlayer()
        FadeToBlack.Apply(1.0)
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If akTarget == Game.GetPlayer()
        WakeUp.Apply(1.0)
    EndIf
EndEvent
