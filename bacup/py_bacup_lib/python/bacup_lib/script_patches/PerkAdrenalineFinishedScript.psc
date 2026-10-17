; The kill count resets only when the buff has lapsed, not when a fresh kill re-cast replaced it.
Event OnEffectFinish(Actor akTarget, Actor akCaster)
    MagicEffect buff = GetBaseObject()
    Utility.Wait(1.0)
    If akTarget != None && NumKillsAV != None && !akTarget.HasMagicEffect(buff)
        akTarget.SetValue(NumKillsAV, 0.0)
    EndIf
EndEvent
