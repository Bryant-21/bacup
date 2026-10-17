; The kill count resets only when the buff has lapsed, not when a fresh kill re-cast replaced it.
Event OnEffectFinish(Actor akTarget, Actor akCaster)
    MagicEffect buff = GetBaseObject()
    Utility.Wait(1.0)
    If akTarget != None && !akTarget.HasMagicEffect(buff)
        If NumKillsAV != None
            akTarget.SetValue(NumKillsAV, 0.0)
        EndIf
        If ExitSoundEffect != None
            ExitSoundEffect.Play(akTarget)
        EndIf
    EndIf
EndEvent
