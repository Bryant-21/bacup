; ImageSpaceModifier.GetDuration is FO76-only and FO4 cannot read an IMAD duration. The
; four bound Babylon storm imods last 2-3 seconds, so the re-apply loop uses a 3 second
; period plus the authored fDelay. Local-player gating becomes a player comparison.

Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget == Game.GetPlayer()
        IModToApply.Apply(1.0)
        StartTimer(3.0 + fDelay, 0)
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If akTarget == Game.GetPlayer()
        CancelTimer(0)
        IModToApply.Remove()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    IModToApply.Apply(1.0)
    StartTimer(3.0 + fDelay, 0)
EndEvent
