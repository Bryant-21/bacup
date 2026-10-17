; The package runs while ConditionalAV is 0; writing AVValueToSet after
; TimerDuration ends the flee. A non-positive DisableCheckDuration skips the
; later check that disables a fled actor once no player is within range.
Event OnStart(Actor akActor)
    myActor = akActor
    If TimerDuration > 0.0
        StartTimer(TimerDuration, FleeTimerID)
    Else
        EndFlee()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == FleeTimerID
        EndFlee()
    ElseIf aiTimerID == DisableTimerID
        DisableIfAlone()
    EndIf
EndEvent

Function EndFlee()
    If myActor == None
        Return
    EndIf
    If ConditionalAV != None
        myActor.SetValue(ConditionalAV, AVValueToSet)
        myActor.EvaluatePackage()
    EndIf
    If DisableIfNoPlayersAround && DisableCheckDuration > 0.0
        StartTimer(DisableCheckDuration, DisableTimerID)
    EndIf
EndFunction

Function DisableIfAlone()
    If myActor == None || myActor.IsDead() || myActor.IsDisabled() || PlayerCheckRadius <= 0.0
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || playerRef.GetDistance(myActor) > PlayerCheckRadius
        myActor.Disable()
        myActor = None
    Else
        StartTimer(DisableCheckDuration, DisableTimerID)
    EndIf
EndFunction
