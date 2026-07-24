Event OnEffectStart(Actor akTarget, Actor akCaster)
    EffectOwner = akTarget
    StartTimer(Utility.RandomFloat(CooldownMin, CooldownMax), CooldownTimerID)
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    CancelTimer(CooldownTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == CooldownTimerID && EffectOwner && !EffectOwner.IsDead()
        EffectOwner.PlaceAtMe(ExplosionForm)
        StartTimer(Utility.RandomFloat(CooldownMin, CooldownMax), CooldownTimerID)
    EndIf
EndEvent
