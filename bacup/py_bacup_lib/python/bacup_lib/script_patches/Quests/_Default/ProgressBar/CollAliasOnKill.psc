; TargetFaction narrows which members count (XPD_RTTP01 pairs it with a negative
; ModPercentage, so killing the wrong side costs progress).
Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    Actor victim = akSenderRef as Actor
    If victim == None || !CanContribute()
        Return
    EndIf
    If TargetFaction != None && !victim.IsInFaction(TargetFaction)
        Return
    EndIf
    ContributeProgress(1.0, akSenderRef)
EndEvent
