Event OnEffectStart(Actor akTarget, Actor akCaster)
    theDeathclaw = akTarget
    StartTimer(5.0, 1)
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    CancelTimer(1)
    CancelTimer(2)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        myHandler = theDeathclaw.GetLinkedRef(HandlerToDeathclawLinkKeyword) as Actor
        If myHandler == None || myHandler.IsDead() || theDeathclaw.GetDistance(myHandler) > RadiusForHandlerToBeInToLink
            If DeathclawFaction && theDeathclaw.IsInFaction(DeathclawFaction)
                theDeathclaw.RemoveFromFaction(DeathclawFaction)
            EndIf
            theDeathclaw.PlayIdle(Deathclaw_Roar)
            StartTimer(idleLength, 2)
        Else
            StartTimer(5.0, 1)
        EndIf
    ElseIf aiTimerID == 2
        If Aggression
            theDeathclaw.SetValue(Aggression, Frenzied)
        EndIf
    EndIf
EndEvent
