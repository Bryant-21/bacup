Event OnPowerOn(ObjectReference akPowerGenerator)
    If ExplosionToPlay == None || GetState() == "playing"
        Return
    EndIf

    If delayTime > 0.0
        GoToState("playing")
        StartTimer(delayTime, delayTimerID)
    Else
        PlaceAtMe(ExplosionToPlay)
        GoToState("readytoplay")
    EndIf
EndEvent

Event OnPowerOff()
    CancelTimer(delayTimerID)
    GoToState("readytoplay")
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != delayTimerID
        Return
    EndIf

    If GetState() == "playing" && IsPowered() && ExplosionToPlay != None
        PlaceAtMe(ExplosionToPlay)
    EndIf

    GoToState("readytoplay")
EndEvent
